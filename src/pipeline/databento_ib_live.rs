//! Live Trading with Databento Data + Interactive Brokers Execution
//!
//! Uses Databento's live streaming API for real-time tick data with accurate
//! buy/sell attribution (delta), and IB for order execution.
//!
//! This module uses the STATE MACHINE approach for real-time LVN detection:
//! 1. Detect breakouts using daily levels (PDH/PDL/VAH/VAL)
//! 2. Profile impulse legs in real-time
//! 3. Extract LVNs from those impulses
//! 4. Hunt for retest with delta confirmation
//!
//! This is the production live trading module.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Timelike};
use databento::{
    dbn::{Schema, SType, TradeMsg},
    live::Subscription,
    LiveClient,
};
use std::sync::Arc;
use tracing::{info, warn, error, debug};

use ibapi::Client as IbClient;

use crate::bars::Bar;
use crate::trades::{Trade, Side};
use super::ib_live::{IbConfig, IbOrderManager, create_nq_contract};
use super::lvn_retest::Direction;
use super::live_trader::{LiveConfig, LiveTrader, TradeAction};
use super::state_machine::{StateMachineConfig, LiveDailyLevels};
use super::precompute;

/// Aggregates trades into 1-second bars with accurate delta
struct BarAggregator {
    current_bar: Option<BarBuilder>,
    symbol: String,
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
}

impl BarBuilder {
    fn new(timestamp: DateTime<Utc>, price: f64, size: u64, is_buy: bool) -> Self {
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

    fn to_bar(&self, symbol: &str) -> Bar {
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
            symbol: symbol.to_string(),
        }
    }
}

impl BarAggregator {
    fn new(symbol: String) -> Self {
        Self {
            current_bar: None,
            symbol,
        }
    }

    fn process_trade(
        &mut self,
        timestamp: DateTime<Utc>,
        price: f64,
        size: u64,
        is_buy: bool,
    ) -> Option<Bar> {
        let second = timestamp.timestamp();

        match &mut self.current_bar {
            Some(bar) => {
                let bar_second = bar.timestamp.timestamp();
                if second > bar_second {
                    let completed = bar.to_bar(&self.symbol);
                    self.current_bar = Some(BarBuilder::new(timestamp, price, size, is_buy));
                    Some(completed)
                } else {
                    bar.add_trade(price, size, is_buy);
                    None
                }
            }
            None => {
                self.current_bar = Some(BarBuilder::new(timestamp, price, size, is_buy));
                None
            }
        }
    }
}

/// Run live trading with Databento data and IB execution (State Machine Mode)
pub async fn run_databento_ib_live(
    api_key: String,
    config: LiveConfig,
    sm_config: StateMachineConfig,
    ib_config: IbConfig,
    paper_mode: bool,
) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("  LIVE TRADING - DATABENTO + IB (State Machine Mode)       ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");
    info!("Data Source: Databento GLBX.MDP3 (tick-level with delta)");
    info!("Execution: Interactive Brokers");
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
    info!("State Machine Config:");
    info!("  Breakout threshold: {:.1} pts", sm_config.breakout_threshold);
    info!("  Min impulse size: {:.1} pts", sm_config.min_impulse_size);
    info!("  Min impulse score: {}", sm_config.min_impulse_score);
    info!("");

    // Initialize trader with state machine
    let mut trader = LiveTrader::new_with_state_machine(config.clone(), sm_config);

    // Load daily levels from cache (most recent day)
    info!("Loading daily levels from {:?}...", config.cache_dir);
    match load_daily_levels_from_cache(&config.cache_dir) {
        Ok(levels) => {
            info!("Loaded daily levels: PDH={:.2} PDL={:.2} VAH={:.2} VAL={:.2}",
                levels.pdh, levels.pdl, levels.vah, levels.val);
            trader.set_daily_levels(levels);
        }
        Err(e) => {
            warn!("Could not load daily levels from cache: {}", e);
            warn!("State machine will wait for levels to be set manually or computed from session data");
        }
    }

    // Connect to IB for execution
    let ib_connection_url = format!("{}:{}", ib_config.host, ib_config.port);
    info!("Connecting to IB at {}...", ib_connection_url);

    let ib_client = IbClient::connect(&ib_connection_url, ib_config.client_id)
        .context("Failed to connect to IB TWS/Gateway")?;

    let ib_client = Arc::new(ib_client);
    info!("Connected to IB");

    // Create contract and order manager
    let contract = create_nq_contract();
    let mut order_manager = IbOrderManager::new(ib_client.clone(), contract);

    // Connect to Databento for live data
    info!("Connecting to Databento...");

    let mut databento_client = LiveClient::builder()
        .key(api_key)?
        .dataset("GLBX.MDP3")
        .build()
        .await
        .context("Failed to connect to Databento")?;

    info!("Connected to Databento");

    // Subscribe to NQ trades
    // Use the specific front month contract (March 2026)
    let symbol = "NQH6".to_string(); // NQ March 2026
    let subscription = Subscription::builder()
        .symbols(vec![symbol.clone()])
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    databento_client
        .subscribe(subscription)
        .await
        .context("Failed to subscribe to Databento")?;

    info!("Subscribed to: {}", symbol);

    // Start streaming
    databento_client.start().await.context("Failed to start Databento stream")?;

    info!("");
    info!("═══════════════════════════════════════════════════════════");
    info!("                    TRADING STARTED                        ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");

    // Track state
    let mut current_direction: Option<Direction> = None;
    let mut bar_aggregator = BarAggregator::new(config.symbol.clone());
    let mut trade_count = 0u64;
    let mut bar_count = 0u64;
    let mut last_status_time = std::time::Instant::now();

    // Track RTH session for computing next day's levels
    let mut rth_high = f64::NEG_INFINITY;
    let mut rth_low = f64::INFINITY;
    let mut last_rth_update_hour: Option<u32> = None;

    // Process incoming trades
    while let Some(record) = databento_client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            trade_count += 1;

            // Determine buy/sell from aggressor side
            // 'A' = Ask side (buyer aggressor = BUY), 'B' = Bid side (seller aggressor = SELL)
            let is_buy = match trade.side as u8 {
                b'A' | b'a' => true,
                b'B' | b'b' => false,
                _ => true, // Default to buy
            };

            // Convert price from fixed-point
            let price = trade.price as f64 / 1_000_000_000.0;
            let size = trade.size as u64;

            // Convert timestamp
            let timestamp = DateTime::from_timestamp_nanos(trade.hd.ts_event as i64);

            // Get hour in ET (approximate: UTC-5)
            let utc_hour = timestamp.hour();
            let et_hour = (utc_hour + 24 - 5) % 24;

            // Track RTH high/low (9:30-16:00 ET)
            if et_hour >= 9 && et_hour < 16 {
                rth_high = rth_high.max(price);
                rth_low = rth_low.min(price);
            }

            // Update levels for evening session when we cross into post-market
            // At 17:00 ET, use today's RTH as the new levels
            if et_hour == 17 && last_rth_update_hour != Some(17) && rth_high > f64::NEG_INFINITY {
                let evening_levels = LiveDailyLevels {
                    date: timestamp.date_naive(),
                    pdh: rth_high,
                    pdl: rth_low,
                    onh: rth_high,
                    onl: rth_low,
                    vah: rth_high - (rth_high - rth_low) * 0.3,
                    val: rth_low + (rth_high - rth_low) * 0.3,
                    session_high: rth_high,
                    session_low: rth_low,
                };
                trader.set_daily_levels(evening_levels);
                info!("Updated evening levels from RTH: PDH={:.2} PDL={:.2}", rth_high, rth_low);
                last_rth_update_hour = Some(17);
            }

            // Feed trade to state machine if profiling impulse
            if trader.is_profiling_impulse() {
                let side = if is_buy { Side::Buy } else { Side::Sell };
                let trade_data = Trade {
                    ts_event: timestamp,
                    price,
                    size,
                    side,
                    symbol: symbol.clone(),
                };
                trader.process_trade(&trade_data);
            }

            // Aggregate into 1-second bars
            if let Some(bar) = bar_aggregator.process_trade(timestamp, price, size, is_buy) {
                bar_count += 1;

                debug!(
                    "Bar #{}: {} | O={:.2} H={:.2} L={:.2} C={:.2} | V={} Delta={}",
                    bar_count,
                    bar.timestamp.format("%H:%M:%S"),
                    bar.open, bar.high, bar.low, bar.close,
                    bar.volume, bar.delta
                );

                // Check if we just started profiling (breakout bar)
                let was_profiling = trader.is_profiling_impulse();

                // Process bar through trader
                if let Some(action) = trader.process_bar(&bar) {
                    match action {
                        TradeAction::Enter { direction, price, stop, target, contracts } => {
                            current_direction = Some(direction);
                            info!(
                                "SIGNAL: {} @ {:.2} | Stop: {:.2} | Target: {:.2} | Delta: {}",
                                if matches!(direction, Direction::Long) { "BUY" } else { "SELL" },
                                price, stop, target, bar.delta
                            );

                            if let Err(e) = order_manager.submit_bracket_order(
                                direction, contracts, stop, target,
                            ) {
                                error!("Order failed: {}", e);
                            }
                        }
                        TradeAction::Exit { pnl_points, reason, .. } => {
                            info!("EXIT: {} | P&L: {:.2} pts", reason, pnl_points);
                            order_manager.clear_orders();
                            current_direction = None;
                        }
                        TradeAction::UpdateStop { new_stop } => {
                            if let Some(dir) = current_direction {
                                debug!("Updating stop to {:.2}", new_stop);
                                let _ = order_manager.modify_stop(new_stop, config.contracts, dir);
                            }
                        }
                        TradeAction::FlattenAll { reason } => {
                            warn!("Flatten: {}", reason);
                            let _ = order_manager.flatten_all();
                            current_direction = None;
                            break; // Exit trading loop
                        }
                        TradeAction::SignalPending => {}
                    }
                }

                // If we just started profiling, the first bar's trades were already processed above
                if !was_profiling && trader.is_profiling_impulse() {
                    debug!("Started impulse profiling");
                }
            }

            // Print status every 30 seconds
            if last_status_time.elapsed() > std::time::Duration::from_secs(30) {
                info!(
                    "Trades: {} | Bars: {} | {}",
                    trade_count, bar_count, trader.status()
                );
                last_status_time = std::time::Instant::now();
            }
        }
    }

    warn!("Databento stream ended");
    info!("Final status: {}", trader.status());

    Ok(())
}

/// Load daily levels from the most recent cached day
fn load_daily_levels_from_cache(cache_dir: &std::path::Path) -> Result<LiveDailyLevels> {
    // Load all cached days
    let days = precompute::load_all_cached(cache_dir, None)?;

    if days.is_empty() {
        anyhow::bail!("No cached data found");
    }

    // Get the most recent day
    let yesterday = days.last().unwrap();

    // Compute high/low from bars
    let high = yesterday.bars_1s.iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = yesterday.bars_1s.iter()
        .map(|b| b.low)
        .fold(f64::INFINITY, f64::min);

    let today = chrono::Utc::now().date_naive();

    Ok(LiveDailyLevels {
        date: today,
        pdh: high,
        pdl: low,
        onh: high, // Simplified: use session high/low
        onl: low,
        vah: high - (high - low) * 0.3, // Approximate VAH
        val: low + (high - low) * 0.3,  // Approximate VAL
        session_high: high,
        session_low: low,
    })
}
