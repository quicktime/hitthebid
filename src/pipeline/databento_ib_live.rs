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
use hitthebid::topstepx::TopstepExecutor;
use super::ib_execution::{IbConfig, IbOrderManager, create_nq_contract_with_symbol};
use super::lvn_retest::Direction;
use super::trader::{LiveConfig, LiveTrader, TradeAction};
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
    contract_symbol: String,
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
    info!("Contract: {} ({} on {})", contract_symbol, config.symbol, config.exchange);
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
    let contract = create_nq_contract_with_symbol(&contract_symbol);
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

    // Subscribe to NQ trades using the specified contract
    let subscription = Subscription::builder()
        .symbols(vec![contract_symbol.clone()])
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    databento_client
        .subscribe(subscription)
        .await
        .context("Failed to subscribe to Databento")?;

    info!("Subscribed to: {}", contract_symbol);

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
                    symbol: contract_symbol.clone(),
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

/// Run in observe mode - no execution, just track trades and log to file
/// This is for paper trading without broker connection or for generating alerts
pub async fn run_observe_mode(
    api_key: String,
    contract_symbol: String,
    config: LiveConfig,
    sm_config: StateMachineConfig,
    trade_log: std::path::PathBuf,
) -> Result<()> {
    use std::io::Write;

    println!("═══════════════════════════════════════════════════════════");
    println!("           OBSERVE MODE - PAPER TRADING                    ");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("📊 Data Source: Databento GLBX.MDP3");
    println!("📝 Trade Log: {:?}", trade_log);
    println!("📈 Contract: {}", contract_symbol);
    println!("⏰ Trading Hours: {:02}:{:02} - {:02}:{:02} ET",
        config.start_hour, config.start_minute,
        config.end_hour, config.end_minute
    );
    println!();
    println!("🔔 Alerts will print here for MANUAL execution");
    println!("   Use R|Trader Pro to place bracket orders");
    println!();

    // Initialize trader with state machine
    let mut trader = LiveTrader::new_with_state_machine(config.clone(), sm_config);

    // Load daily levels from cache
    info!("Loading daily levels from {:?}...", config.cache_dir);
    match load_daily_levels_from_cache(&config.cache_dir) {
        Ok(levels) => {
            println!("✓ Loaded daily levels:");
            println!("  PDH: {:.2}  PDL: {:.2}", levels.pdh, levels.pdl);
            println!("  VAH: {:.2}  VAL: {:.2}", levels.vah, levels.val);
            trader.set_daily_levels(levels);
        }
        Err(e) => {
            warn!("Could not load daily levels: {}", e);
        }
    }

    // Initialize trade log file
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trade_log)
        .context("Failed to open trade log file")?;

    // Write header if file is empty
    if log_file.metadata()?.len() == 0 {
        writeln!(log_file, "timestamp,action,direction,price,stop,target,pnl_points,reason")?;
    }

    // Connect to Databento
    println!();
    println!("Connecting to Databento...");

    let mut databento_client = LiveClient::builder()
        .key(api_key)?
        .dataset("GLBX.MDP3")
        .build()
        .await
        .context("Failed to connect to Databento")?;

    let subscription = Subscription::builder()
        .symbols(vec![contract_symbol.clone()])
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    databento_client.subscribe(subscription).await?;
    databento_client.start().await?;

    println!("✓ Connected to Databento");
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                 OBSERVING MARKET                          ");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    // Track state
    let mut bar_aggregator = BarAggregator::new(config.symbol.clone());
    let mut trade_count = 0u64;
    let mut bar_count = 0u64;
    let mut signal_count = 0u32;
    let mut last_status_time = std::time::Instant::now();

    // Track current simulated position for internal P&L
    let mut current_entry: Option<(Direction, f64, DateTime<Utc>)> = None;
    let mut total_pnl = 0.0f64;
    let mut wins = 0u32;
    let mut losses = 0u32;

    // Track RTH session
    let mut rth_high = f64::NEG_INFINITY;
    let mut rth_low = f64::INFINITY;
    let mut last_rth_update_hour: Option<u32> = None;

    // Process incoming trades
    while let Some(record) = databento_client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            trade_count += 1;

            let is_buy = match trade.side as u8 {
                b'A' | b'a' => true,
                b'B' | b'b' => false,
                _ => true,
            };

            let price = trade.price as f64 / 1_000_000_000.0;
            let size = trade.size as u64;
            let timestamp = DateTime::from_timestamp_nanos(trade.hd.ts_event as i64);

            let utc_hour = timestamp.hour();
            let et_hour = (utc_hour + 24 - 5) % 24;

            // Track RTH high/low
            if et_hour >= 9 && et_hour < 16 {
                rth_high = rth_high.max(price);
                rth_low = rth_low.min(price);
            }

            // Update evening levels
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
                println!("📊 Updated levels from RTH: PDH={:.2} PDL={:.2}", rth_high, rth_low);
                last_rth_update_hour = Some(17);
            }

            // Feed trade to state machine if profiling
            if trader.is_profiling_impulse() {
                let side = if is_buy { Side::Buy } else { Side::Sell };
                let trade_data = Trade {
                    ts_event: timestamp,
                    price,
                    size,
                    side,
                    symbol: contract_symbol.clone(),
                };
                trader.process_trade(&trade_data);
            }

            // Aggregate into bars
            if let Some(bar) = bar_aggregator.process_trade(timestamp, price, size, is_buy) {
                bar_count += 1;

                let was_profiling = trader.is_profiling_impulse();

                // Process bar through trader
                if let Some(action) = trader.process_bar(&bar) {
                    let now = chrono::Local::now().format("%H:%M:%S");

                    match action {
                        TradeAction::Enter { direction, price: entry_price, stop, target, contracts: _ } => {
                            signal_count += 1;
                            current_entry = Some((direction, entry_price, timestamp));

                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };
                            let dir_emoji = if matches!(direction, Direction::Long) { "🟢" } else { "🔴" };

                            // Print prominent alert
                            println!();
                            println!("╔═══════════════════════════════════════════════════════════╗");
                            println!("║  {} SIGNAL #{}: {} @ {:.2}                      ║", dir_emoji, signal_count, dir_str, entry_price);
                            println!("╠═══════════════════════════════════════════════════════════╣");
                            println!("║  🎯 Entry:  {:.2}                                        ║", entry_price);
                            println!("║  🛑 Stop:   {:.2}                                        ║", stop);
                            println!("║  ✅ Target: {:.2}                                        ║", target);
                            println!("║  📊 Delta:  {}                                           ║", bar.delta);
                            println!("╚═══════════════════════════════════════════════════════════╝");
                            println!("  ⏰ Time: {} | Execute on R|Trader Pro!", now);
                            println!();

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},{:.2},{:.2},,",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "ENTRY", dir_str, entry_price, stop, target
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::Exit { direction, price: exit_price, pnl_points, reason } => {
                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };

                            total_pnl += pnl_points;
                            if pnl_points > 0.0 {
                                wins += 1;
                                println!("✅ EXIT {} @ {:.2} | +{:.2} pts | {} | Total: {:.2} pts",
                                    dir_str, exit_price, pnl_points, reason, total_pnl);
                            } else {
                                losses += 1;
                                println!("❌ EXIT {} @ {:.2} | {:.2} pts | {} | Total: {:.2} pts",
                                    dir_str, exit_price, pnl_points, reason, total_pnl);
                            }

                            current_entry = None;

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},,,{:.2},{}",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "EXIT", dir_str, exit_price, pnl_points, reason
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::UpdateStop { new_stop } => {
                            debug!("Stop updated to {:.2}", new_stop);
                        }
                        TradeAction::FlattenAll { reason } => {
                            println!("⚠️  FLATTEN ALL: {}", reason);
                            current_entry = None;
                            break;
                        }
                        TradeAction::SignalPending => {}
                    }
                }

                if !was_profiling && trader.is_profiling_impulse() {
                    debug!("Started impulse profiling");
                }
            }

            // Status every 60 seconds
            if last_status_time.elapsed() > std::time::Duration::from_secs(60) {
                let pos_status = if current_entry.is_some() { "IN POSITION" } else { "FLAT" };
                println!("📈 {} | Bars: {} | Signals: {} | W:{} L:{} | P&L: {:.2} pts",
                    pos_status, bar_count, signal_count, wins, losses, total_pnl);
                last_status_time = std::time::Instant::now();
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                    SESSION COMPLETE                        ");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total Signals: {}", signal_count);
    println!("Wins: {} | Losses: {}", wins, losses);
    println!("Total P&L: {:.2} pts", total_pnl);
    println!("Trade log saved to: {:?}", trade_log);

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

/// Live trading with Databento data + TopstepX execution
pub async fn run_topstep_mode(
    api_key: String,
    contract_symbol: String,
    config: LiveConfig,
    sm_config: StateMachineConfig,
    mut executor: TopstepExecutor,
    trade_log: std::path::PathBuf,
) -> Result<()> {
    use std::io::Write;
    use hitthebid::topstepx::{Direction as TsDirection, TradeAction as TsTradeAction};

    println!("═══════════════════════════════════════════════════════════");
    println!("           TOPSTEP LIVE TRADING                            ");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("📊 Data Source: Databento GLBX.MDP3");
    println!("🏦 Execution: TopstepX API");
    println!("📝 Trade Log: {:?}", trade_log);
    println!("📈 Contract: {}", contract_symbol);
    println!("⏰ Trading Hours: {:02}:{:02} - {:02}:{:02} ET",
        config.start_hour, config.start_minute,
        config.end_hour, config.end_minute
    );
    println!();

    // Initialize trader with state machine
    let mut trader = LiveTrader::new_with_state_machine(config.clone(), sm_config);

    // Load daily levels from cache
    info!("Loading daily levels from {:?}...", config.cache_dir);
    match load_daily_levels_from_cache(&config.cache_dir) {
        Ok(levels) => {
            println!("✓ Loaded daily levels:");
            println!("  PDH: {:.2}  PDL: {:.2}", levels.pdh, levels.pdl);
            println!("  VAH: {:.2}  VAL: {:.2}", levels.vah, levels.val);
            trader.set_daily_levels(levels);
        }
        Err(e) => {
            warn!("Could not load daily levels: {}", e);
        }
    }

    // Initialize trade log file
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trade_log)
        .context("Failed to open trade log file")?;

    // Write header if file is empty
    if log_file.metadata()?.len() == 0 {
        writeln!(log_file, "timestamp,action,direction,price,stop,target,pnl_points,reason,executed")?;
    }

    // Connect to Databento
    println!();
    println!("Connecting to Databento...");

    let mut databento_client = LiveClient::builder()
        .key(api_key)?
        .dataset("GLBX.MDP3")
        .build()
        .await
        .context("Failed to connect to Databento")?;

    let subscription = Subscription::builder()
        .symbols(vec![contract_symbol.clone()])
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    databento_client.subscribe(subscription).await?;
    databento_client.start().await?;

    println!("✓ Connected to Databento");
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                 TRADING ACTIVE                            ");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    // Track state
    let mut bar_aggregator = BarAggregator::new(config.symbol.clone());
    let mut trade_count = 0u64;
    let mut bar_count = 0u64;
    let mut signal_count = 0u32;
    let mut last_status_time = std::time::Instant::now();

    // Track P&L
    let mut total_pnl = 0.0f64;
    let mut wins = 0u32;
    let mut losses = 0u32;

    // Track RTH session
    let mut rth_high = f64::NEG_INFINITY;
    let mut rth_low = f64::INFINITY;
    let mut last_rth_update_hour: Option<u32> = None;

    // Process incoming trades
    while let Some(record) = databento_client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            trade_count += 1;

            let is_buy = match trade.side as u8 {
                b'A' | b'a' => true,
                b'B' | b'b' => false,
                _ => true,
            };

            let price = trade.price as f64 / 1_000_000_000.0;
            let size = trade.size as u64;
            let timestamp = DateTime::from_timestamp_nanos(trade.hd.ts_event as i64);

            let utc_hour = timestamp.hour();
            let et_hour = (utc_hour + 24 - 5) % 24;

            // Track RTH high/low
            if et_hour >= 9 && et_hour < 16 {
                rth_high = rth_high.max(price);
                rth_low = rth_low.min(price);
            }

            // Update evening levels
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
                println!("📊 Updated levels from RTH: PDH={:.2} PDL={:.2}", rth_high, rth_low);
                last_rth_update_hour = Some(17);
            }

            // Feed trade to state machine if profiling
            if trader.is_profiling_impulse() {
                let side = if is_buy { Side::Buy } else { Side::Sell };
                let trade_data = Trade {
                    ts_event: timestamp,
                    price,
                    size,
                    side,
                    symbol: contract_symbol.clone(),
                };
                trader.process_trade(&trade_data);
            }

            // Aggregate into bars
            if let Some(bar) = bar_aggregator.process_trade(timestamp, price, size, is_buy) {
                bar_count += 1;

                // Process bar through trader
                if let Some(action) = trader.process_bar(&bar) {
                    let now = chrono::Local::now().format("%H:%M:%S");

                    match action {
                        TradeAction::Enter { direction, price: entry_price, stop, target, contracts } => {
                            signal_count += 1;

                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };
                            let dir_emoji = if matches!(direction, Direction::Long) { "🟢" } else { "🔴" };

                            // Print signal
                            println!();
                            println!("╔═══════════════════════════════════════════════════════════╗");
                            println!("║  {} SIGNAL #{}: {} @ {:.2}                      ║", dir_emoji, signal_count, dir_str, entry_price);
                            println!("╠═══════════════════════════════════════════════════════════╣");
                            println!("║  🎯 Entry:  {:.2}                                        ║", entry_price);
                            println!("║  🛑 Stop:   {:.2}                                        ║", stop);
                            println!("║  ✅ Target: {:.2}                                        ║", target);
                            println!("║  📊 Delta:  {}                                           ║", bar.delta);
                            println!("╚═══════════════════════════════════════════════════════════╝");

                            // Convert to TopstepX types and execute
                            let ts_direction = if matches!(direction, Direction::Long) {
                                TsDirection::Long
                            } else {
                                TsDirection::Short
                            };

                            let ts_action = TsTradeAction::Enter {
                                direction: ts_direction,
                                price: entry_price,
                                stop,
                                target,
                                contracts,
                            };

                            let executed = match executor.execute(ts_action).await {
                                Ok(_) => {
                                    println!("  ✅ ORDER EXECUTED via TopstepX | {}", now);
                                    true
                                }
                                Err(e) => {
                                    error!("  ❌ EXECUTION FAILED: {} | {}", e, now);
                                    false
                                }
                            };
                            println!();

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},{:.2},{:.2},,{}",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "ENTRY", dir_str, entry_price, stop, target, executed
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::Exit { direction, price: exit_price, pnl_points, reason } => {
                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };

                            total_pnl += pnl_points;

                            // Execute exit via TopstepX
                            let ts_direction = if matches!(direction, Direction::Long) {
                                TsDirection::Long
                            } else {
                                TsDirection::Short
                            };

                            let ts_action = TsTradeAction::Exit {
                                direction: ts_direction,
                                price: exit_price,
                                pnl_points,
                                reason: reason.clone(),
                            };

                            let executed = match executor.execute(ts_action).await {
                                Ok(_) => true,
                                Err(e) => {
                                    error!("Exit execution failed: {}", e);
                                    false
                                }
                            };

                            if pnl_points > 0.0 {
                                wins += 1;
                                println!("✅ EXIT {} @ {:.2} | +{:.2} pts | {} | Total: {:.2} pts | Exec: {}",
                                    dir_str, exit_price, pnl_points, reason, total_pnl, executed);
                            } else {
                                losses += 1;
                                println!("❌ EXIT {} @ {:.2} | {:.2} pts | {} | Total: {:.2} pts | Exec: {}",
                                    dir_str, exit_price, pnl_points, reason, total_pnl, executed);
                            }

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},,,{:.2},{},{}",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "EXIT", dir_str, exit_price, pnl_points, reason, executed
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::UpdateStop { new_stop } => {
                            debug!("Updating stop to {:.2}", new_stop);
                            let ts_action = TsTradeAction::UpdateStop { new_stop };
                            if let Err(e) = executor.execute(ts_action).await {
                                warn!("Failed to update stop: {}", e);
                            }
                        }
                        TradeAction::FlattenAll { reason } => {
                            println!("⚠️  FLATTEN ALL: {}", reason);
                            let ts_action = TsTradeAction::FlattenAll { reason: reason.clone() };
                            if let Err(e) = executor.execute(ts_action).await {
                                error!("Failed to flatten: {}", e);
                            }
                            break;
                        }
                        TradeAction::SignalPending => {}
                    }
                }
            }

            // Status every 60 seconds
            if last_status_time.elapsed() > std::time::Duration::from_secs(60) {
                let pos_info = executor.position_info();
                let pos_status = if pos_info.is_some() { "IN POSITION" } else { "FLAT" };
                println!("📈 {} | Bars: {} | Signals: {} | W:{} L:{} | P&L: {:.2} pts",
                    pos_status, bar_count, signal_count, wins, losses, total_pnl);

                // Sync position state periodically
                if let Err(e) = executor.sync_position().await {
                    warn!("Position sync failed: {}", e);
                }

                last_status_time = std::time::Instant::now();
            }
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                    SESSION COMPLETE                        ");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total Signals: {}", signal_count);
    println!("Wins: {} | Losses: {}", wins, losses);
    println!("Total P&L: {:.2} pts", total_pnl);
    println!("Trade log saved to: {:?}", trade_log);

    Ok(())
}

/// Run live trading with Databento data + Rithmic execution
pub async fn run_rithmic_mode(
    api_key: String,
    contract_symbol: String,
    config: LiveConfig,
    sm_config: StateMachineConfig,
    mut rithmic_conn: hitthebid::execution::RithmicConnection,
    trade_log: std::path::PathBuf,
) -> Result<()> {
    use std::io::Write;

    println!("═══════════════════════════════════════════════════════════");
    println!("           RITHMIC LIVE TRADING                             ");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("📊 Data Source: Databento GLBX.MDP3");
    println!("🏦 Execution: Rithmic API");
    println!("📝 Trade Log: {:?}", trade_log);
    println!("📈 Contract: {}", contract_symbol);
    println!("⏰ Trading Hours: {:02}:{:02} - {:02}:{:02} ET",
        config.start_hour, config.start_minute,
        config.end_hour, config.end_minute
    );
    println!();

    // Initialize trader with state machine
    let mut trader = LiveTrader::new_with_state_machine(config.clone(), sm_config);

    // Load daily levels from cache
    info!("Loading daily levels from {:?}...", config.cache_dir);
    match load_daily_levels_from_cache(&config.cache_dir) {
        Ok(levels) => {
            println!("✓ Loaded daily levels:");
            println!("  PDH: {:.2}  PDL: {:.2}", levels.pdh, levels.pdl);
            println!("  VAH: {:.2}  VAL: {:.2}", levels.vah, levels.val);
            trader.set_daily_levels(levels);
        }
        Err(e) => {
            warn!("Could not load daily levels: {}", e);
        }
    }

    // Initialize trade log file
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trade_log)
        .context("Failed to open trade log file")?;

    // Write header if file is empty
    if log_file.metadata()?.len() == 0 {
        writeln!(log_file, "timestamp,action,direction,price,stop,target,pnl_points,reason,executed")?;
    }

    // Connect to Databento
    println!();
    println!("Connecting to Databento...");

    let mut databento_client = LiveClient::builder()
        .key(api_key)?
        .dataset("GLBX.MDP3")
        .build()
        .await
        .context("Failed to connect to Databento")?;

    let subscription = Subscription::builder()
        .symbols(vec![contract_symbol.clone()])
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    databento_client.subscribe(subscription).await?;
    databento_client.start().await?;

    println!("✓ Connected to Databento");
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                 TRADING ACTIVE                            ");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    // Track state
    let mut bar_aggregator = BarAggregator::new(config.symbol.clone());
    let mut _trade_count = 0u64;
    let mut bar_count = 0u64;
    let mut signal_count = 0u32;
    let mut last_status_time = std::time::Instant::now();

    // Track P&L
    let mut total_pnl = 0.0f64;
    let mut wins = 0u32;
    let mut losses = 0u32;

    // Track RTH session
    let mut rth_high = f64::NEG_INFINITY;
    let mut rth_low = f64::INFINITY;
    let mut last_rth_update_hour: Option<u32> = None;

    // Track active order for stop modifications
    let mut active_order_id: Option<String> = None;

    // Process incoming trades
    while let Some(record) = databento_client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            _trade_count += 1;

            let is_buy = match trade.side as u8 {
                b'A' | b'a' => true,
                b'B' | b'b' => false,
                _ => true,
            };

            let price = trade.price as f64 / 1_000_000_000.0;
            let size = trade.size as u64;
            let timestamp = DateTime::from_timestamp_nanos(trade.hd.ts_event as i64);

            let utc_hour = timestamp.hour();
            let et_hour = (utc_hour + 24 - 5) % 24;

            // Track RTH high/low
            if et_hour >= 9 && et_hour < 16 {
                rth_high = rth_high.max(price);
                rth_low = rth_low.min(price);
            }

            // Update evening levels
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
                println!("📊 Updated levels from RTH: PDH={:.2} PDL={:.2}", rth_high, rth_low);
                last_rth_update_hour = Some(17);
            }

            // Feed trade to state machine if profiling
            if trader.is_profiling_impulse() {
                let side = if is_buy { Side::Buy } else { Side::Sell };
                let trade_data = Trade {
                    ts_event: timestamp,
                    price,
                    size,
                    side,
                    symbol: contract_symbol.clone(),
                };
                trader.process_trade(&trade_data);
            }

            // Aggregate into bars
            if let Some(bar) = bar_aggregator.process_trade(timestamp, price, size, is_buy) {
                bar_count += 1;

                // Process bar through trader
                if let Some(action) = trader.process_bar(&bar) {
                    let now = chrono::Local::now().format("%H:%M:%S");

                    match action {
                        TradeAction::Enter { direction, price: entry_price, stop, target, contracts } => {
                            signal_count += 1;

                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };
                            let dir_emoji = if matches!(direction, Direction::Long) { "🟢" } else { "🔴" };
                            let side_str = if matches!(direction, Direction::Long) { "BUY" } else { "SELL" };

                            // Print signal
                            println!();
                            println!("╔═══════════════════════════════════════════════════════════╗");
                            println!("║  {} SIGNAL #{}: {} @ {:.2}                      ║", dir_emoji, signal_count, dir_str, entry_price);
                            println!("╠═══════════════════════════════════════════════════════════╣");
                            println!("║  🎯 Entry:  {:.2}                                        ║", entry_price);
                            println!("║  🛑 Stop:   {:.2}                                        ║", stop);
                            println!("║  ✅ Target: {:.2}                                        ║", target);
                            println!("║  📊 Delta:  {}                                           ║", bar.delta);
                            println!("╚═══════════════════════════════════════════════════════════╝");

                            // Calculate stop and profit in ticks (NQ/MNQ tick = 0.25)
                            let tick_size = 0.25;
                            let stop_distance = (entry_price - stop).abs();
                            let profit_distance = (target - entry_price).abs();
                            let stop_ticks = (stop_distance / tick_size).round() as i32;
                            let profit_ticks = (profit_distance / tick_size).round() as i32;

                            // Execute via Rithmic bracket order
                            let executed = match rithmic_conn.submit_bracket_order(
                                &contract_symbol,
                                "CME",
                                side_str,
                                contracts,
                                stop_ticks,
                                profit_ticks,
                                None, // Market entry
                            ).await {
                                Ok(order_id) => {
                                    println!("  ✅ ORDER EXECUTED via Rithmic [{}] | {}", order_id, now);
                                    active_order_id = Some(order_id);
                                    true
                                }
                                Err(e) => {
                                    error!("  ❌ EXECUTION FAILED: {} | {}", e, now);
                                    false
                                }
                            };
                            println!();

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},{:.2},{:.2},,{}",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "ENTRY", dir_str, entry_price, stop, target, executed
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::Exit { direction, price: exit_price, pnl_points, reason } => {
                            let dir_str = if matches!(direction, Direction::Long) { "LONG" } else { "SHORT" };

                            total_pnl += pnl_points;

                            // Exit position via Rithmic
                            let executed = match rithmic_conn.exit_position(&contract_symbol, "CME").await {
                                Ok(_) => {
                                    active_order_id = None;
                                    true
                                }
                                Err(e) => {
                                    error!("Exit execution failed: {}", e);
                                    false
                                }
                            };

                            if pnl_points > 0.0 {
                                wins += 1;
                                println!("✅ EXIT {} @ {:.2} | +{:.2} pts | {} | Total: {:.2} pts | Exec: {}",
                                    dir_str, exit_price, pnl_points, reason, total_pnl, executed);
                            } else {
                                losses += 1;
                                println!("❌ EXIT {} @ {:.2} | {:.2} pts | {} | Total: {:.2} pts | Exec: {}",
                                    dir_str, exit_price, pnl_points, reason, total_pnl, executed);
                            }

                            // Log to file
                            writeln!(log_file, "{},{},{},{:.2},,,{:.2},{},{}",
                                timestamp.format("%Y-%m-%d %H:%M:%S"),
                                "EXIT", dir_str, exit_price, pnl_points, reason, executed
                            )?;
                            log_file.flush()?;
                        }
                        TradeAction::UpdateStop { new_stop } => {
                            debug!("Updating stop to {:.2}", new_stop);
                            if let Some(ref order_id) = active_order_id {
                                // Calculate new stop in ticks relative to current price
                                let tick_size = 0.25;
                                let new_stop_ticks = ((price - new_stop).abs() / tick_size).round() as i32;

                                if let Err(e) = rithmic_conn.modify_stop(order_id, new_stop_ticks).await {
                                    warn!("Failed to update stop: {}", e);
                                }
                            }
                        }
                        TradeAction::FlattenAll { reason } => {
                            println!("⚠️  FLATTEN ALL: {}", reason);
                            if let Err(e) = rithmic_conn.cancel_all_orders().await {
                                error!("Failed to cancel orders: {}", e);
                            }
                            if let Err(e) = rithmic_conn.exit_position(&contract_symbol, "CME").await {
                                error!("Failed to exit position: {}", e);
                            }
                            active_order_id = None;
                            break;
                        }
                        TradeAction::SignalPending => {}
                    }
                }
            }

            // Status every 60 seconds
            if last_status_time.elapsed() > std::time::Duration::from_secs(60) {
                let pos_status = if active_order_id.is_some() { "IN POSITION" } else { "FLAT" };
                println!("📈 {} | Bars: {} | Signals: {} | W:{} L:{} | P&L: {:.2} pts",
                    pos_status, bar_count, signal_count, wins, losses, total_pnl);
                last_status_time = std::time::Instant::now();
            }
        }
    }

    // Clean disconnect
    if let Err(e) = rithmic_conn.disconnect().await {
        warn!("Error during disconnect: {}", e);
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("                    SESSION COMPLETE                        ");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total Signals: {}", signal_count);
    println!("Wins: {} | Losses: {}", wins, losses);
    println!("Total P&L: {:.2} pts", total_pnl);
    println!("Trade log saved to: {:?}", trade_log);

    Ok(())
}
