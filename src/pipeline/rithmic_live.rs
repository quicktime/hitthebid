//! Live Trading via Rithmic API
//!
//! Connects to Rithmic for both market data and order execution.
//! Uses the same signal generation logic as the backtester.

use anyhow::{Result, bail, Context};
use chrono::{DateTime, Utc, Timelike, Local};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

use rithmic_rs::{
    RithmicConfig, RithmicEnv, ConnectStrategy,
    RithmicTickerPlant, RithmicOrderPlant, RithmicOrderPlantHandle,
};
use rithmic_rs::rti::messages::RithmicMessage;
use rithmic_rs::ws::RithmicStream;
use rithmic_rs::api::rithmic_command_types::RithmicBracketOrder;

use crate::bars::Bar;
use super::lvn_retest::{LvnRetestConfig, LvnSignalGenerator, Direction, LvnSignal};
use super::precompute;

/// Configuration for live trading
#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// Symbol to trade (e.g., "NQ" for E-mini Nasdaq)
    pub symbol: String,
    /// Exchange (e.g., "CME")
    pub exchange: String,
    /// Number of contracts to trade
    pub contracts: i32,
    /// Cache directory for LVN levels
    pub cache_dir: PathBuf,
    /// Take profit in points
    pub take_profit: f64,
    /// Trailing stop distance in points
    pub trailing_stop: f64,
    /// Stop buffer beyond LVN level in points
    pub stop_buffer: f64,
    /// Trading start hour (ET, 24h format)
    pub start_hour: u32,
    /// Trading start minute
    pub start_minute: u32,
    /// Trading end hour (ET, 24h format)
    pub end_hour: u32,
    /// Trading end minute
    pub end_minute: u32,
    /// Minimum delta for absorption signal
    pub min_delta: i64,
    /// Maximum LVN volume ratio
    pub max_lvn_ratio: f64,
    /// Level tolerance in points
    pub level_tolerance: f64,
    /// Starting balance for tracking
    pub starting_balance: f64,
    /// Max daily losses before stopping
    pub max_daily_losses: i32,
    /// Daily P&L loss limit in points
    pub daily_loss_limit: f64,
    /// Point value (NQ = $20)
    pub point_value: f64,
}

impl LiveConfig {
    /// Build the LVN strategy config from the flat config
    pub fn to_lvn_config(&self) -> LvnRetestConfig {
        LvnRetestConfig {
            level_tolerance: self.level_tolerance,
            retest_distance: 8.0,
            min_delta_for_absorption: self.min_delta,
            max_range_for_absorption: 1.5,
            stop_loss: self.stop_buffer,
            take_profit: self.take_profit,
            trailing_stop: self.trailing_stop,
            max_hold_bars: 300,
            rth_only: true,
            cooldown_bars: 60,
            level_cooldown_bars: 600,
            max_lvn_volume_ratio: self.max_lvn_ratio,
            same_day_only: false,
            min_absorption_bars: 1,
            structure_stop_buffer: self.stop_buffer,
            trade_start_hour: self.start_hour,
            trade_start_minute: self.start_minute,
            trade_end_hour: self.end_hour,
            trade_end_minute: self.end_minute,
        }
    }
}

/// Aggregates trades into 1-second bars
struct BarAggregator {
    current_bar: Option<BarBuilder>,
    completed_bars: Vec<Bar>,
}

struct BarBuilder {
    timestamp: DateTime<Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: u64,
    buy_volume: u64,
    sell_volume: u64,
    trade_count: u64,
    symbol: String,
}

impl BarBuilder {
    fn new(timestamp: DateTime<Utc>, price: f64, size: u64, is_buy: bool, symbol: String) -> Self {
        let (buy_vol, sell_vol) = if is_buy { (size, 0) } else { (0, size) };
        Self {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: size,
            buy_volume: buy_vol,
            sell_volume: sell_vol,
            trade_count: 1,
            symbol,
        }
    }

    fn add_trade(&mut self, price: f64, size: u64, is_buy: bool) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.volume += size;
        if is_buy {
            self.buy_volume += size;
        } else {
            self.sell_volume += size;
        }
        self.trade_count += 1;
    }

    fn to_bar(&self) -> Bar {
        Bar {
            timestamp: self.timestamp,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
            buy_volume: self.buy_volume,
            sell_volume: self.sell_volume,
            delta: self.buy_volume as i64 - self.sell_volume as i64,
            trade_count: self.trade_count,
            symbol: self.symbol.clone(),
        }
    }
}

impl BarAggregator {
    fn new() -> Self {
        Self {
            current_bar: None,
            completed_bars: Vec::new(),
        }
    }

    /// Process a trade and return completed bar if a new second started
    fn process_trade(
        &mut self,
        timestamp: DateTime<Utc>,
        price: f64,
        size: u64,
        is_buy: bool,
        symbol: &str,
    ) -> Option<Bar> {
        let second = timestamp.timestamp();

        match &mut self.current_bar {
            Some(bar) => {
                let bar_second = bar.timestamp.timestamp();
                if second > bar_second {
                    // New second - complete current bar and start new one
                    let completed = bar.to_bar();
                    self.current_bar = Some(BarBuilder::new(timestamp, price, size, is_buy, symbol.to_string()));
                    self.completed_bars.push(completed.clone());
                    Some(completed)
                } else {
                    // Same second - add to current bar
                    bar.add_trade(price, size, is_buy);
                    None
                }
            }
            None => {
                // First trade
                self.current_bar = Some(BarBuilder::new(timestamp, price, size, is_buy, symbol.to_string()));
                None
            }
        }
    }
}

/// Tracks open position during live trading
#[derive(Debug)]
struct OpenPosition {
    direction: Direction,
    entry_price: f64,
    entry_time: DateTime<Utc>,
    level_price: f64,
    initial_stop: f64,
    take_profit: f64,
    trailing_stop: f64,
    highest_price: f64,
    lowest_price: f64,
    bar_count: usize,
}

/// Live trading state
pub struct LiveTrader {
    config: LiveConfig,
    lvn_config: LvnRetestConfig,
    signal_gen: LvnSignalGenerator,
    bar_aggregator: BarAggregator,
    pending_signal: Option<LvnSignal>,
    open_position: Option<OpenPosition>,

    // Statistics
    daily_losses: i32,
    daily_pnl: f64,
    total_trades: i32,
    wins: i32,
    losses: i32,
    running_balance: f64,

    // State
    is_trading_hours: bool,
    daily_stopped: bool,
    bar_count: usize,
}

impl LiveTrader {
    pub fn new(config: LiveConfig) -> Self {
        let lvn_config = config.to_lvn_config();
        let signal_gen = LvnSignalGenerator::new(lvn_config.clone());
        let starting_balance = config.starting_balance;

        Self {
            config,
            lvn_config,
            signal_gen,
            bar_aggregator: BarAggregator::new(),
            pending_signal: None,
            open_position: None,
            daily_losses: 0,
            daily_pnl: 0.0,
            total_trades: 0,
            wins: 0,
            losses: 0,
            running_balance: starting_balance,
            is_trading_hours: false,
            daily_stopped: false,
            bar_count: 0,
        }
    }

    /// Load LVN levels from cache
    pub fn load_lvn_levels(&mut self, cache_dir: &PathBuf) -> Result<usize> {
        let days = precompute::load_all_cached(cache_dir, None)?;
        let mut total_levels = 0;
        for day in &days {
            self.signal_gen.add_lvn_levels(&day.lvn_levels);
            total_levels += day.lvn_levels.len();
        }
        Ok(total_levels)
    }

    /// Check if current time is within trading hours
    fn check_trading_hours(&mut self) {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        let start = self.lvn_config.trade_start_hour * 60
            + self.lvn_config.trade_start_minute;
        let end = self.lvn_config.trade_end_hour * 60
            + self.lvn_config.trade_end_minute;
        let current = hour * 60 + minute;

        self.is_trading_hours = current >= start && current < end;
    }

    /// Process a completed bar
    pub fn process_bar(&mut self, bar: &Bar) -> Option<TradeAction> {
        self.bar_count += 1;
        self.check_trading_hours();

        // Check if we should stop for the day
        if self.daily_stopped {
            return None;
        }

        // Check daily loss limit
        if self.daily_pnl <= -self.config.daily_loss_limit {
            warn!("Daily loss limit reached: {:.2} pts", self.daily_pnl);
            self.daily_stopped = true;
            return Some(TradeAction::FlattenAll { reason: "Daily loss limit".to_string() });
        }

        // Step 1: If we have a pending signal, enter now
        if let Some(signal) = self.pending_signal.take() {
            if self.daily_stopped || !self.is_trading_hours {
                info!("Skipping signal - not trading hours or stopped");
                return None;
            }

            let entry_price = bar.open;
            let level_price = signal.level_price;

            let (initial_stop, take_profit) = match signal.direction {
                Direction::Long => (
                    level_price - self.lvn_config.structure_stop_buffer,
                    entry_price + self.lvn_config.take_profit,
                ),
                Direction::Short => (
                    level_price + self.lvn_config.structure_stop_buffer,
                    entry_price - self.lvn_config.take_profit,
                ),
            };

            self.open_position = Some(OpenPosition {
                direction: signal.direction,
                entry_price,
                entry_time: bar.timestamp,
                level_price,
                initial_stop,
                take_profit,
                trailing_stop: initial_stop,
                highest_price: entry_price,
                lowest_price: entry_price,
                bar_count: 0,
            });

            info!(
                "ENTRY: {:?} @ {:.2} | Stop: {:.2} | Target: {:.2}",
                signal.direction, entry_price, initial_stop, take_profit
            );

            return Some(TradeAction::Enter {
                direction: signal.direction,
                price: entry_price,
                stop: initial_stop,
                target: take_profit,
                contracts: self.config.contracts,
            });
        }

        // Step 2: Manage open position
        if let Some(ref mut pos) = self.open_position {
            pos.bar_count += 1;
            pos.highest_price = pos.highest_price.max(bar.high);
            pos.lowest_price = pos.lowest_price.min(bar.low);

            // Update trailing stop
            let activation_distance = self.lvn_config.trailing_stop;
            match pos.direction {
                Direction::Long => {
                    if pos.highest_price >= pos.entry_price + activation_distance {
                        let new_trail = pos.highest_price - self.lvn_config.trailing_stop;
                        if new_trail > pos.trailing_stop {
                            pos.trailing_stop = new_trail;
                            debug!("Trailing stop updated to {:.2}", new_trail);
                        }
                    }
                }
                Direction::Short => {
                    if pos.lowest_price <= pos.entry_price - activation_distance {
                        let new_trail = pos.lowest_price + self.lvn_config.trailing_stop;
                        if new_trail < pos.trailing_stop {
                            pos.trailing_stop = new_trail;
                            debug!("Trailing stop updated to {:.2}", new_trail);
                        }
                    }
                }
            }

            // Check for exit
            let mut should_exit = false;
            let mut exit_price = bar.close;
            let mut exit_reason = "Unknown";

            match pos.direction {
                Direction::Long => {
                    if bar.low <= pos.trailing_stop {
                        should_exit = true;
                        exit_price = pos.trailing_stop;
                        exit_reason = "STOP";
                    } else if bar.high >= pos.take_profit {
                        should_exit = true;
                        exit_price = pos.take_profit;
                        exit_reason = "TARGET";
                    }
                }
                Direction::Short => {
                    if bar.high >= pos.trailing_stop {
                        should_exit = true;
                        exit_price = pos.trailing_stop;
                        exit_reason = "STOP";
                    } else if bar.low <= pos.take_profit {
                        should_exit = true;
                        exit_price = pos.take_profit;
                        exit_reason = "TARGET";
                    }
                }
            }

            // Check timeout
            if !should_exit && pos.bar_count >= self.lvn_config.max_hold_bars {
                should_exit = true;
                exit_price = bar.close;
                exit_reason = "TIMEOUT";
            }

            if should_exit {
                let pnl_points = match pos.direction {
                    Direction::Long => exit_price - pos.entry_price,
                    Direction::Short => pos.entry_price - exit_price,
                };

                let pnl_dollars = pnl_points * self.config.point_value * self.config.contracts as f64;
                self.daily_pnl += pnl_points;
                self.running_balance += pnl_dollars;
                self.total_trades += 1;

                if pnl_points > 0.5 {
                    self.wins += 1;
                    info!(
                        "EXIT {}: {:?} @ {:.2} | P&L: +{:.2} pts (${:+.2}) | WIN",
                        exit_reason, pos.direction, exit_price, pnl_points, pnl_dollars
                    );
                } else if pnl_points < -0.5 {
                    self.losses += 1;
                    self.daily_losses += 1;
                    info!(
                        "EXIT {}: {:?} @ {:.2} | P&L: {:.2} pts (${:.2}) | LOSS",
                        exit_reason, pos.direction, exit_price, pnl_points, pnl_dollars
                    );

                    // Check max daily losses
                    if self.config.max_daily_losses > 0
                        && self.daily_losses >= self.config.max_daily_losses
                    {
                        warn!("Max daily losses ({}) reached", self.config.max_daily_losses);
                        self.daily_stopped = true;
                    }
                } else {
                    info!(
                        "EXIT {}: {:?} @ {:.2} | P&L: {:.2} pts | BREAKEVEN",
                        exit_reason, pos.direction, exit_price, pnl_points
                    );
                }

                let direction = pos.direction;
                self.open_position = None;

                return Some(TradeAction::Exit {
                    direction,
                    price: exit_price,
                    pnl_points,
                    reason: exit_reason.to_string(),
                });
            }

            // Return trailing stop update if changed
            return Some(TradeAction::UpdateStop {
                new_stop: pos.trailing_stop,
            });
        }

        // Step 3: Check for new signal (only if flat and trading hours)
        if self.open_position.is_none()
            && self.pending_signal.is_none()
            && self.is_trading_hours
            && !self.daily_stopped
        {
            if let Some(signal) = self.signal_gen.process_bar(bar) {
                info!(
                    "SIGNAL: {:?} @ {:.2} | Level: {:.2} | Delta: {}",
                    signal.direction, signal.price, signal.level_price, signal.delta
                );
                self.pending_signal = Some(signal);
                return Some(TradeAction::SignalPending);
            }
        }

        None
    }

    /// Get current status summary
    pub fn status(&self) -> String {
        let win_rate = if self.total_trades > 0 {
            self.wins as f64 / self.total_trades as f64 * 100.0
        } else {
            0.0
        };

        format!(
            "Balance: ${:.2} | Day P&L: {:.2} pts | Trades: {} | WR: {:.1}% | Position: {}",
            self.running_balance,
            self.daily_pnl,
            self.total_trades,
            win_rate,
            if self.open_position.is_some() { "OPEN" } else { "FLAT" }
        )
    }

    /// Reset for new trading day
    pub fn reset_daily(&mut self) {
        self.daily_losses = 0;
        self.daily_pnl = 0.0;
        self.daily_stopped = false;
        info!("Daily stats reset. Balance: ${:.2}", self.running_balance);
    }

    /// Check if in a position
    pub fn is_flat(&self) -> bool {
        self.open_position.is_none() && self.pending_signal.is_none()
    }
}

/// Actions the trading loop should take
#[derive(Debug, Clone)]
pub enum TradeAction {
    /// Enter a new position
    Enter {
        direction: Direction,
        price: f64,
        stop: f64,
        target: f64,
        contracts: i32,
    },
    /// Exit current position
    Exit {
        direction: Direction,
        price: f64,
        pnl_points: f64,
        reason: String,
    },
    /// Update stop loss
    UpdateStop {
        new_stop: f64,
    },
    /// Signal pending for next bar
    SignalPending,
    /// Flatten all positions
    FlattenAll {
        reason: String,
    },
}

/// Order Manager - handles bracket order submission and modifications
pub struct OrderManager {
    symbol: String,
    exchange: String,
    tick_size: f64,
    order_counter: u64,
    /// Current bracket order ID (basket_id from Rithmic)
    current_bracket_id: Option<String>,
    /// Current stop order ID for modifications
    current_stop_id: Option<String>,
}

impl OrderManager {
    pub fn new(symbol: String, exchange: String, tick_size: f64) -> Self {
        Self {
            symbol,
            exchange,
            tick_size,
            order_counter: 0,
            current_bracket_id: None,
            current_stop_id: None,
        }
    }

    /// Generate unique local order ID
    fn next_order_id(&mut self) -> String {
        self.order_counter += 1;
        format!("LVN_{}", self.order_counter)
    }

    /// Convert price to ticks
    fn price_to_ticks(&self, price: f64) -> i32 {
        (price / self.tick_size).round() as i32
    }

    /// Submit a bracket order (entry + stop + target)
    pub async fn submit_bracket_order(
        &mut self,
        order_handle: &mut RithmicOrderPlantHandle,
        direction: Direction,
        contracts: i32,
        entry_price: f64,
        stop_price: f64,
        target_price: f64,
    ) -> Result<()> {
        let local_id = self.next_order_id();

        // Calculate stop and profit in ticks from entry
        let stop_distance = (entry_price - stop_price).abs();
        let profit_distance = (target_price - entry_price).abs();

        let stop_ticks = self.price_to_ticks(stop_distance);
        let profit_ticks = self.price_to_ticks(profit_distance);

        // Action: 1 = Buy, 2 = Sell
        let action = match direction {
            Direction::Long => 1,
            Direction::Short => 2,
        };

        let bracket_order = RithmicBracketOrder {
            action,
            duration: 2, // Day order
            exchange: self.exchange.clone(),
            localid: local_id.clone(),
            ordertype: 2, // Market order (1 = Limit, 2 = Market)
            price: None,  // Market order doesn't need price
            profit_ticks,
            qty: contracts,
            stop_ticks,
            symbol: self.symbol.clone(),
        };

        info!(
            "Submitting bracket order: {} {} @ MKT | Stop: {} ticks | Target: {} ticks",
            if action == 1 { "BUY" } else { "SELL" },
            contracts,
            stop_ticks,
            profit_ticks
        );

        match order_handle.place_bracket_order(bracket_order).await {
            Ok(responses) => {
                for response in &responses {
                    // Extract basket_id from response for future modifications
                    if let RithmicMessage::ResponseBracketOrder(ref bracket_resp) = response.message {
                        if let Some(ref basket_id) = bracket_resp.basket_id {
                            self.current_bracket_id = Some(basket_id.clone());
                            info!("Bracket order placed: basket_id={}", basket_id);
                        }
                    }
                }
                Ok(())
            }
            Err(e) => {
                error!("Failed to place bracket order: {}", e);
                Err(anyhow::anyhow!("Bracket order failed: {}", e))
            }
        }
    }

    /// Modify the stop loss on an existing bracket order
    pub async fn modify_stop(
        &mut self,
        order_handle: &mut RithmicOrderPlantHandle,
        new_stop_price: f64,
        entry_price: f64,
        direction: Direction,
    ) -> Result<()> {
        let Some(ref bracket_id) = self.current_bracket_id else {
            debug!("No bracket order to modify");
            return Ok(());
        };

        // Calculate new stop in ticks from entry
        let stop_distance = match direction {
            Direction::Long => entry_price - new_stop_price,
            Direction::Short => new_stop_price - entry_price,
        };

        let stop_ticks = self.price_to_ticks(stop_distance);

        debug!("Modifying stop to {} ticks from entry", stop_ticks);

        match order_handle.adjust_stop(bracket_id, stop_ticks).await {
            Ok(_) => {
                debug!("Stop modified successfully");
                Ok(())
            }
            Err(e) => {
                warn!("Failed to modify stop: {}", e);
                Ok(()) // Don't fail the whole system on stop modification error
            }
        }
    }

    /// Cancel all orders and exit position
    pub async fn flatten_all(
        &mut self,
        order_handle: &mut RithmicOrderPlantHandle,
    ) -> Result<()> {
        info!("Flattening all positions...");

        // Cancel all orders first
        if let Err(e) = order_handle.cancel_all_orders().await {
            warn!("Error canceling orders: {}", e);
        }

        // Exit any open position
        if let Err(e) = order_handle.exit_position(&self.symbol, &self.exchange).await {
            warn!("Error exiting position: {}", e);
        }

        self.current_bracket_id = None;
        self.current_stop_id = None;

        info!("Flattened all positions");
        Ok(())
    }

    /// Clear order tracking (called when position is closed)
    pub fn clear_orders(&mut self) {
        self.current_bracket_id = None;
        self.current_stop_id = None;
    }
}

/// Main live trading loop
pub async fn run_live(config: LiveConfig, paper_mode: bool) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("           LIVE TRADING - LVN RETEST STRATEGY              ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");
    info!("Symbol: {} on {}", config.symbol, config.exchange);
    info!("Contracts: {}", config.contracts);
    info!("Mode: {}", if paper_mode { "PAPER" } else { "LIVE" });
    info!("Trading Hours: {:02}:{:02} - {:02}:{:02} ET",
        config.start_hour,
        config.start_minute,
        config.end_hour,
        config.end_minute
    );
    info!("");

    // Initialize trader
    let mut trader = LiveTrader::new(config.clone());

    // Load LVN levels
    info!("Loading LVN levels from {:?}...", config.cache_dir);
    let level_count = trader.load_lvn_levels(&config.cache_dir)?;
    info!("Loaded {} LVN levels", level_count);

    // Configure Rithmic connection
    let rithmic_env = if paper_mode { RithmicEnv::Demo } else { RithmicEnv::Live };

    info!("Connecting to Rithmic {:?}...", rithmic_env);
    let rithmic_config = RithmicConfig::from_env(rithmic_env)
        .context("Failed to load Rithmic config from environment. Set RITHMIC_USER, RITHMIC_PASSWORD, etc.")?;

    // Connect to ticker plant for market data
    let ticker_plant = RithmicTickerPlant::connect(&rithmic_config, ConnectStrategy::Retry).await
        .map_err(|e| anyhow::anyhow!("Failed to connect to Rithmic ticker plant: {}", e))?;
    let mut ticker_handle = ticker_plant.get_handle();

    // Login and subscribe
    ticker_handle.login().await
        .map_err(|e| anyhow::anyhow!("Failed to login to Rithmic: {}", e))?;
    info!("Logged in to Rithmic");

    // Subscribe to market data
    let symbol = format!("{}.c.0", config.symbol); // Front month continuous
    let _ = ticker_handle.subscribe(&symbol, &config.exchange).await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to market data: {}", e))?;
    info!("Subscribed to {} on {}", symbol, config.exchange);

    // Connect to order plant for execution
    let order_plant = RithmicOrderPlant::connect(&rithmic_config, ConnectStrategy::Retry).await
        .map_err(|e| anyhow::anyhow!("Failed to connect to Rithmic order plant: {}", e))?;
    let mut order_handle = order_plant.get_handle();

    // Login to order plant
    order_handle.login().await
        .map_err(|e| anyhow::anyhow!("Failed to login to order plant: {}", e))?;
    info!("Order plant logged in");

    // Subscribe to order and bracket updates
    let _ = order_handle.subscribe_order_updates().await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to order updates: {}", e))?;
    let _ = order_handle.subscribe_bracket_updates().await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to bracket updates: {}", e))?;
    info!("Subscribed to order updates");

    // Initialize order manager (NQ tick size = 0.25)
    let tick_size = 0.25; // NQ tick size
    let mut order_manager = OrderManager::new(symbol.clone(), config.exchange.clone(), tick_size);

    // Track entry price for stop modifications
    let mut current_entry_price: Option<f64> = None;
    let mut current_direction: Option<Direction> = None;

    // Main trading loop
    info!("");
    info!("═══════════════════════════════════════════════════════════");
    info!("                    TRADING STARTED                        ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");

    let mut bar_aggregator = BarAggregator::new();
    let mut last_status_time = std::time::Instant::now();

    loop {
        // Receive market data
        match ticker_handle.subscription_receiver.recv().await {
            Ok(update) => {
                match update.message {
                    RithmicMessage::LastTrade(trade) => {
                        // Extract trade data
                        let price = trade.trade_price.unwrap_or(0.0);
                        let size = trade.trade_size.unwrap_or(0) as u64;
                        let is_buy = trade.aggressor.map(|s| s == 1).unwrap_or(true);
                        let timestamp = Utc::now(); // Use current time for live

                        // Aggregate into bar
                        if let Some(bar) = bar_aggregator.process_trade(timestamp, price, size, is_buy, &symbol) {
                            // Process completed bar
                            if let Some(action) = trader.process_bar(&bar) {
                                match action {
                                    TradeAction::Enter { direction, price, stop, target, contracts } => {
                                        // Submit bracket order
                                        current_entry_price = Some(price);
                                        current_direction = Some(direction);

                                        if let Err(e) = order_manager.submit_bracket_order(
                                            &mut order_handle,
                                            direction,
                                            contracts,
                                            price,
                                            stop,
                                            target,
                                        ).await {
                                            error!("Failed to submit bracket order: {}", e);
                                        }
                                    }
                                    TradeAction::Exit { direction: _, price: _, pnl_points, reason } => {
                                        // Exit handled by bracket order - just log and clear state
                                        info!("Position closed: {} | P&L: {:.2} pts", reason, pnl_points);
                                        order_manager.clear_orders();
                                        current_entry_price = None;
                                        current_direction = None;
                                    }
                                    TradeAction::UpdateStop { new_stop } => {
                                        // Modify stop order
                                        if let (Some(entry), Some(dir)) = (current_entry_price, current_direction) {
                                            if let Err(e) = order_manager.modify_stop(
                                                &mut order_handle,
                                                new_stop,
                                                entry,
                                                dir,
                                            ).await {
                                                warn!("Failed to modify stop: {}", e);
                                            }
                                        }
                                    }
                                    TradeAction::FlattenAll { reason } => {
                                        warn!("Flattening all: {}", reason);
                                        if let Err(e) = order_manager.flatten_all(&mut order_handle).await {
                                            error!("Failed to flatten: {}", e);
                                        }
                                        current_entry_price = None;
                                        current_direction = None;
                                        break;
                                    }
                                    TradeAction::SignalPending => {
                                        // Signal will be executed on next bar
                                    }
                                }
                            }
                        }
                    }
                    RithmicMessage::HeartbeatTimeout => {
                        warn!("Rithmic heartbeat timeout - reconnecting");
                        break;
                    }
                    RithmicMessage::ForcedLogout(msg) => {
                        error!("Forced logout: {:?}", msg);
                        break;
                    }
                    RithmicMessage::ConnectionError => {
                        error!("Connection error");
                        break;
                    }
                    _ => {}
                }
            }
            Err(e) => {
                error!("Channel error: {}", e);
                break;
            }
        }

        // Print status every 30 seconds
        if last_status_time.elapsed() > std::time::Duration::from_secs(30) {
            info!("{}", trader.status());
            last_status_time = std::time::Instant::now();
        }
    }

    info!("Trading loop ended");
    info!("Final status: {}", trader.status());

    Ok(())
}
