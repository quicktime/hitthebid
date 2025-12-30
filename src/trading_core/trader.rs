//! Core Trading Logic
//!
//! Broker-agnostic trading state machine used by both:
//! - Replay testing (historical data from cache)
//! - Live trading (Databento feed + execution)
//!
//! This module contains no broker-specific code.

use anyhow::Result;
use chrono::{DateTime, Utc, Timelike};
use std::path::PathBuf;
use tracing::{info, warn, debug};

use super::bars::Bar;
use super::lvn_retest::{LvnRetestConfig, LvnSignalGenerator, Direction, LvnSignal};
use super::cache;
use super::state_machine::{TradingStateMachine, StateMachineConfig, StateTransition, LiveDailyLevels};
use super::trades::Trade;
use super::lvn::LvnLevel;

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
    /// Slippage per trade in points (applied to entry and exit)
    pub slippage: f64,
    /// Commission per round-trip in dollars
    pub commission: f64,
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

/// Live trading state - used for both live trading and replay testing
pub struct LiveTrader {
    config: LiveConfig,
    lvn_config: LvnRetestConfig,
    signal_gen: LvnSignalGenerator,
    bar_aggregator: BarAggregator,
    pending_signal: Option<LvnSignal>,
    open_position: Option<OpenPosition>,

    // State machine for real-time breakout detection (optional)
    state_machine: Option<TradingStateMachine>,
    use_state_machine: bool,

    // Statistics
    daily_losses: i32,
    daily_pnl: f64,
    total_trades: i32,
    wins: i32,
    losses: i32,
    breakevens: i32,
    running_balance: f64,
    peak_balance: f64,
    max_drawdown: f64,
    gross_profit: f64,  // Sum of winning trade P&L (points)
    gross_loss: f64,    // Sum of losing trade P&L (points, positive value)
    trade_pnls: Vec<f64>, // Individual trade P&Ls for Sharpe calculation
    total_commission: f64, // Total commission paid
    total_slippage: f64,   // Total slippage cost in points

    // State
    is_trading_hours: bool,
    daily_stopped: bool,
    bar_count: usize,

    // Date tracking for daily resets
    current_date: Option<chrono::NaiveDate>,
    days_stopped_early: u32,
    signals_skipped: u32,

    // Track which impulse generated the current trade (for clearing LVNs after trade)
    current_trade_impulse_id: Option<uuid::Uuid>,
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
            state_machine: None,
            use_state_machine: false,
            daily_losses: 0,
            daily_pnl: 0.0,
            total_trades: 0,
            wins: 0,
            losses: 0,
            breakevens: 0,
            running_balance: starting_balance,
            peak_balance: starting_balance,
            max_drawdown: 0.0,
            gross_profit: 0.0,
            gross_loss: 0.0,
            trade_pnls: Vec::new(),
            total_commission: 0.0,
            total_slippage: 0.0,
            is_trading_hours: false,
            daily_stopped: false,
            bar_count: 0,
            current_date: None,
            days_stopped_early: 0,
            signals_skipped: 0,
            current_trade_impulse_id: None,
        }
    }

    /// Create a new LiveTrader with state machine enabled for real-time breakout detection
    pub fn new_with_state_machine(config: LiveConfig, sm_config: StateMachineConfig) -> Self {
        let mut trader = Self::new(config);
        trader.state_machine = Some(TradingStateMachine::new(sm_config));
        trader.use_state_machine = true;
        trader
    }

    /// Set daily levels for the state machine
    pub fn set_daily_levels(&mut self, levels: LiveDailyLevels) {
        if let Some(ref mut sm) = self.state_machine {
            sm.set_daily_levels(levels);
        }
    }

    /// Feed a trade to the state machine (for LVN extraction during impulse profiling)
    pub fn process_trade(&mut self, trade: &Trade) {
        if let Some(ref mut sm) = self.state_machine {
            sm.process_trade(trade);
        }
    }

    /// Check if the state machine is currently profiling an impulse
    pub fn is_profiling_impulse(&self) -> bool {
        if let Some(ref sm) = self.state_machine {
            sm.state() == super::state_machine::TradingState::ProfilingImpulse
        } else {
            false
        }
    }

    /// Add LVN levels directly (for replay mode)
    pub fn add_lvn_levels(&mut self, levels: &[LvnLevel]) {
        self.signal_gen.add_lvn_levels(levels);
    }

    /// Clear all LVN levels (for new day in replay)
    pub fn clear_levels(&mut self) {
        self.signal_gen.clear_levels();
    }

    /// Reset for new trading day in replay (close any open position, clear levels, reset daily stats)
    pub fn reset_for_new_day(&mut self, last_price: Option<f64>) {
        // Close any open position at EOD (count as timeout/scratch)
        if let Some(pos) = self.open_position.take() {
            let exit_price = last_price.unwrap_or(pos.entry_price);
            let pnl_points = match pos.direction {
                Direction::Long => exit_price - pos.entry_price,
                Direction::Short => pos.entry_price - exit_price,
            };

            // Update stats
            self.total_trades += 1;
            self.running_balance += pnl_points * self.config.point_value * self.config.contracts as f64;

            if pnl_points > 1.0 {
                self.wins += 1;
                self.gross_profit += pnl_points;
            } else if pnl_points < -1.0 {
                self.losses += 1;
                self.gross_loss += pnl_points.abs();
            } else {
                self.breakevens += 1;
            }

            info!("EOD CLOSE: {:?} @ {:.2} | P&L: {:.2} pts", pos.direction, exit_price, pnl_points);
        }

        // Clear pending signals too
        self.pending_signal = None;
        self.current_trade_impulse_id = None;

        self.clear_levels();
        self.reset_daily();

        // Reset state machine for new day
        if let Some(ref mut sm) = self.state_machine {
            sm.reset_for_new_day();
        }
    }

    /// Load LVN levels from cache - ONLY yesterday's levels (for live trading)
    pub fn load_lvn_levels(&mut self, cache_dir: &PathBuf) -> Result<usize> {
        let days = cache::load_all_cached(cache_dir, None)?;

        // Only load the MOST RECENT day's levels (yesterday's precompute)
        if let Some(yesterday) = days.last() {
            self.signal_gen.add_lvn_levels(&yesterday.lvn_levels);
            info!("Loaded {} LVN levels from {}", yesterday.lvn_levels.len(), yesterday.date);
            Ok(yesterday.lvn_levels.len())
        } else {
            Ok(0)
        }
    }

    /// Check if timestamp is within trading hours (uses bar timestamp for replay, wall clock for live)
    fn check_trading_hours_for_bar(&mut self, bar: &Bar) {
        // Use Eastern Time for trading hours check
        use chrono_tz::America::New_York;
        let et_time = bar.timestamp.with_timezone(&New_York);
        let hour = et_time.hour();
        let minute = et_time.minute();

        let start = self.lvn_config.trade_start_hour * 60
            + self.lvn_config.trade_start_minute;
        let end = self.lvn_config.trade_end_hour * 60
            + self.lvn_config.trade_end_minute;
        let current = hour * 60 + minute;

        self.is_trading_hours = current >= start && current < end;
    }

    /// Check for new day and reset daily counters
    fn check_date_change(&mut self, bar: &Bar) {
        let bar_date = bar.timestamp.date_naive();

        if self.current_date != Some(bar_date) {
            // Count the stopped day before resetting
            if self.daily_stopped {
                self.days_stopped_early += 1;
            }

            // Reset for new day
            self.current_date = Some(bar_date);
            self.daily_losses = 0;
            self.daily_pnl = 0.0;
            self.daily_stopped = false;
        }
    }

    /// Process a completed bar
    pub fn process_bar(&mut self, bar: &Bar) -> Option<TradeAction> {
        self.bar_count += 1;
        self.check_date_change(bar);
        self.check_trading_hours_for_bar(bar);

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

        // Process state machine if enabled (for real-time breakout detection)
        if self.use_state_machine {
            if let Some(ref mut sm) = self.state_machine {
                if let Some(transition) = sm.process_bar(bar) {
                    match transition {
                        StateTransition::BreakoutDetected { level, direction, price } => {
                            info!(
                                "STATE: BREAKOUT {} @ {:.2} | Direction: {:?}",
                                level, price, direction
                            );
                        }
                        StateTransition::ImpulseComplete { impulse_id, lvn_count, direction } => {
                            // Add the newly extracted LVNs to the signal generator
                            let lvns = sm.active_lvns();
                            let added_count = self.signal_gen.add_lvn_levels_with_impulse(lvns, impulse_id);
                            info!(
                                "STATE: IMPULSE COMPLETE | {} LVNs (added {}) | Direction: {:?} | ID: {}",
                                lvn_count, added_count, direction, impulse_id
                            );
                            for lvn in lvns {
                                debug!("  LVN @ {:.2} (vol_ratio: {:.3})", lvn.price, lvn.volume_ratio);
                            }
                        }
                        StateTransition::ImpulseInvalid { reason } => {
                            info!("STATE: IMPULSE INVALID | {}", reason);
                        }
                        StateTransition::HuntingTimeout => {
                            info!("STATE: HUNTING TIMEOUT - resetting");
                            // Clear LVNs from this impulse
                            if let Some(impulse_id) = sm.active_impulse_id() {
                                self.signal_gen.clear_impulse_lvns(impulse_id);
                            }
                        }
                        StateTransition::Reset => {
                            debug!("STATE: RESET - waiting for breakout");
                        }
                    }
                }
            }
        }

        // Step 1: If we have a pending signal, enter now
        if let Some(signal) = self.pending_signal.take() {
            if self.daily_stopped {
                self.signals_skipped += 1;
                info!("SKIPPED: {:?} signal (max daily losses reached)", signal.direction);
                return None;
            }
            if !self.is_trading_hours {
                info!("Skipping signal - not trading hours");
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
                // Calculate gross P&L (before costs)
                let gross_pnl_points = match pos.direction {
                    Direction::Long => exit_price - pos.entry_price,
                    Direction::Short => pos.entry_price - exit_price,
                };

                // Apply slippage (affects both entry and exit)
                let slippage_cost = self.config.slippage * 2.0; // Entry + exit slippage
                let pnl_points = gross_pnl_points - slippage_cost;
                self.total_slippage += slippage_cost;

                // Calculate dollar P&L and apply commission
                let gross_pnl_dollars = pnl_points * self.config.point_value * self.config.contracts as f64;
                let commission = self.config.commission * self.config.contracts as f64;
                let pnl_dollars = gross_pnl_dollars - commission;
                self.total_commission += commission;

                self.daily_pnl += pnl_points;
                self.running_balance += pnl_dollars;
                self.total_trades += 1;
                self.trade_pnls.push(pnl_points);

                // Update peak balance and max drawdown
                if self.running_balance > self.peak_balance {
                    self.peak_balance = self.running_balance;
                }
                let drawdown = self.peak_balance - self.running_balance;
                if drawdown > self.max_drawdown {
                    self.max_drawdown = drawdown;
                }

                if pnl_points > 0.5 {
                    self.wins += 1;
                    self.gross_profit += pnl_points;
                    info!(
                        "EXIT {}: {:?} @ {:.2} | P&L: +{:.2} pts (${:+.2}) | WIN",
                        exit_reason, pos.direction, exit_price, pnl_points, pnl_dollars
                    );
                } else if pnl_points < -0.5 {
                    self.losses += 1;
                    self.daily_losses += 1;
                    self.gross_loss += pnl_points.abs();
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
                    self.breakevens += 1;
                    info!(
                        "EXIT {}: {:?} @ {:.2} | P&L: {:.2} pts | BREAKEVEN",
                        exit_reason, pos.direction, exit_price, pnl_points
                    );
                }

                let direction = pos.direction;
                self.open_position = None;

                // Clear LVNs from this impulse if using state machine mode
                if self.use_state_machine {
                    if let Some(impulse_id) = self.current_trade_impulse_id.take() {
                        info!("Clearing LVNs from impulse {} after trade exit", impulse_id);
                        self.signal_gen.clear_impulse_lvns(impulse_id);
                        // Also reset the state machine to wait for next breakout
                        if let Some(ref mut sm) = self.state_machine {
                            sm.reset();
                        }
                    }
                }

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

        // Step 3: ALWAYS process bar through signal generator to update level states
        // This is critical - level states need to track price movement even during positions
        let signal = self.signal_gen.process_bar(bar);

        // Only act on signal if we're flat and conditions are met
        if signal.is_some()
            && self.open_position.is_none()
            && self.pending_signal.is_none()
            && self.is_trading_hours
            && !self.daily_stopped
        {
            let signal = signal.unwrap();
            info!(
                "SIGNAL: {:?} @ {:.2} | Level: {:.2} | Delta: {}",
                signal.direction, signal.price, signal.level_price, signal.delta
            );

            // Capture the impulse ID for this signal's level (for clearing after trade)
            let level_key = (signal.level_price * 10.0) as i64;
            self.current_trade_impulse_id = self.signal_gen.get_level_impulse_id(level_key);

            self.pending_signal = Some(signal);
            return Some(TradeAction::SignalPending);
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

    /// Get full summary for replay results
    pub fn summary(&self) -> TradingSummary {
        let total = self.total_trades as u32;
        let wins = self.wins as u32;
        let losses = self.losses as u32;
        let breakevens = self.breakevens as u32;

        let win_rate = if total > 0 { wins as f64 / total as f64 * 100.0 } else { 0.0 };

        // Net P&L has slippage already subtracted (from trade_pnls)
        let net_pnl = self.gross_profit - self.gross_loss;
        // Gross P&L = net + slippage we paid
        let gross_pnl = net_pnl + self.total_slippage;

        // Profit factor from tracked values (based on net P&L per trade)
        let profit_factor = if self.gross_loss > 0.0 {
            self.gross_profit / self.gross_loss
        } else if self.gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let avg_win = if wins > 0 {
            self.gross_profit / wins as f64
        } else {
            0.0
        };

        let avg_loss = if losses > 0 {
            -(self.gross_loss / losses as f64)
        } else {
            0.0
        };

        // Count final day if stopped
        let days_stopped = if self.daily_stopped {
            self.days_stopped_early + 1
        } else {
            self.days_stopped_early
        };

        // Calculate Sharpe ratio (annualized) - based on net P&L per trade
        let sharpe_ratio = if total > 1 && !self.trade_pnls.is_empty() {
            let mean_return = net_pnl / total as f64;
            let variance: f64 = self.trade_pnls.iter()
                .map(|r| (r - mean_return).powi(2))
                .sum::<f64>() / total as f64;
            let std_dev = variance.sqrt();
            if std_dev > 0.0 {
                (mean_return / std_dev) * (252.0_f64).sqrt() // Annualized
            } else {
                0.0
            }
        } else {
            0.0
        };

        TradingSummary {
            total_trades: total,
            wins,
            losses,
            breakevens,
            win_rate,
            profit_factor,
            gross_pnl,
            total_slippage: self.total_slippage,
            total_commission: self.total_commission,
            net_pnl,
            avg_win,
            avg_loss,
            max_drawdown: self.max_drawdown,
            final_balance: self.running_balance,
            days_stopped_early: days_stopped,
            signals_skipped: self.signals_skipped,
            sharpe_ratio,
        }
    }
}

/// Summary of trading results
#[derive(Debug)]
pub struct TradingSummary {
    pub total_trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub breakevens: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub gross_pnl: f64,      // P&L before costs
    pub total_slippage: f64, // Total slippage cost (points)
    pub total_commission: f64, // Total commission cost (dollars)
    pub net_pnl: f64,        // P&L after costs
    pub avg_win: f64,
    pub avg_loss: f64,
    pub max_drawdown: f64,
    pub final_balance: f64,
    pub days_stopped_early: u32,
    pub signals_skipped: u32,
    pub sharpe_ratio: f64,
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
