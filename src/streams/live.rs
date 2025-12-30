use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use databento::{
    dbn::{Record, Schema, SType, TradeMsg},
    live::Subscription,
    LiveClient,
};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::processing::ProcessingState;
use crate::trading_core::{
    Bar, LiveConfig, LiveTrader, Side, StateMachineConfig, TradeAction,
    Trade as TradingTrade, Direction, daily_levels,
};
use crate::types::{AppState, Trade, TradingSignal, WsMessage};

/// CSV Trade Logger for tracking all trading activity
struct TradeLogger {
    file: File,
}

impl TradeLogger {
    fn new(path: &str) -> Result<Self> {
        // Create file if it doesn't exist
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        // Check if file is empty to write header
        let metadata = std::fs::metadata(path)?;
        if metadata.len() == 0 {
            let mut file = OpenOptions::new().write(true).open(path)?;
            writeln!(file, "timestamp,event_type,direction,price,stop,target,pnl_points,reason")?;
        }

        let file = OpenOptions::new().append(true).open(path)?;
        Ok(Self { file })
    }

    fn log_entry(&mut self, ts: DateTime<Utc>, direction: &Direction, price: f64, stop: f64, target: f64) {
        let dir_str = match direction {
            Direction::Long => "LONG",
            Direction::Short => "SHORT",
        };
        let _ = writeln!(
            self.file,
            "{},ENTRY,{},{:.2},{:.2},{:.2},,",
            ts.format("%Y-%m-%d %H:%M:%S"),
            dir_str,
            price,
            stop,
            target
        );
        let _ = self.file.flush();
    }

    fn log_exit(&mut self, ts: DateTime<Utc>, direction: &Direction, price: f64, pnl_points: f64, reason: &str) {
        let dir_str = match direction {
            Direction::Long => "LONG",
            Direction::Short => "SHORT",
        };
        let _ = writeln!(
            self.file,
            "{},EXIT,{},{:.2},,,{:.2},{}",
            ts.format("%Y-%m-%d %H:%M:%S"),
            dir_str,
            price,
            pnl_points,
            reason
        );
        let _ = self.file.flush();
    }

    fn log_stop_update(&mut self, ts: DateTime<Utc>, new_stop: f64) {
        let _ = writeln!(
            self.file,
            "{},STOP_UPDATE,,{:.2},,,,",
            ts.format("%Y-%m-%d %H:%M:%S"),
            new_stop
        );
        let _ = self.file.flush();
    }

    fn log_flatten(&mut self, ts: DateTime<Utc>, reason: &str) {
        let _ = writeln!(
            self.file,
            "{},FLATTEN,,,,,,{}",
            ts.format("%Y-%m-%d %H:%M:%S"),
            reason
        );
        let _ = self.file.flush();
    }
}

/// Live mode: Stream real-time data from Databento
pub async fn run_databento_stream(
    api_key: String,
    symbols: Vec<String>,
    state: Arc<AppState>,
    trading_enabled: bool,
    cache_dir: PathBuf,
) -> Result<()> {
    if trading_enabled {
        info!("Trading signals ENABLED - will generate entry/exit alerts");
    }

    // Fetch daily levels from Databento historical API BEFORE connecting to live stream
    let daily_levels_result = if trading_enabled {
        info!("Fetching fresh daily levels from Databento historical API...");
        match daily_levels::fetch_daily_levels(&api_key, &symbols[0]).await {
            Ok(levels) => {
                info!(
                    "Fetched daily levels: PDH={:.2}, PDL={:.2}, POC={:.2}, VAH={:.2}, VAL={:.2}, ONH={:.2}, ONL={:.2}",
                    levels.pdh, levels.pdl, levels.poc, levels.vah, levels.val, levels.onh, levels.onl
                );
                Some(levels)
            }
            Err(e) => {
                warn!("Failed to fetch daily levels: {:?} - state machine will have no breakout levels", e);
                None
            }
        }
    } else {
        None
    };

    info!("Connecting to Databento...");

    let mut client = LiveClient::builder()
        .key(api_key)?
        .dataset("GLBX.MDP3")
        .build()
        .await
        .context("Failed to connect to Databento")?;

    info!("Connected to Databento");

    // Subscribe to symbols
    let subscription = Subscription::builder()
        .symbols(symbols.clone())
        .schema(Schema::Trades)
        .stype_in(SType::RawSymbol)
        .build();

    client
        .subscribe(subscription)
        .await
        .context("Failed to subscribe")?;

    info!("Subscribed to: {:?}", symbols);

    // Notify clients we're connected
    let _ = state.tx.send(WsMessage::Connected {
        symbols: symbols.clone(),
        mode: state.mode.clone(),
    });

    // Start streaming
    client.start().await.context("Failed to start stream")?;

    // Create processing state with Supabase persistence and AppState for stats sync
    let processing_state = Arc::new(RwLock::new(ProcessingState::new(
        state.supabase.clone(),
        state.session_id,
        Some(state.clone()),
    )));

    // Spawn 1-second aggregation task
    let processing_state_clone = processing_state.clone();
    let tx_clone = state.tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let mut pstate = processing_state_clone.write().await;
            pstate.process_buffer(&tx_clone);

            // Send volume profile every second
            pstate.send_volume_profile(&tx_clone);
        }
    });

    // Create shared LiveTrader for the real LVN retest strategy
    let trader = if trading_enabled {
        // Create LiveConfig with default settings
        let config = LiveConfig {
            symbol: symbols.first().cloned().unwrap_or_else(|| "NQ".to_string()),
            exchange: "CME".to_string(),
            contracts: 1,
            cache_dir: cache_dir.clone(),
            take_profit: 20.0,
            trailing_stop: 6.0,
            stop_buffer: 2.0,
            start_hour: 9,
            start_minute: 30,
            end_hour: 16,
            end_minute: 0,
            min_delta: 100,
            max_lvn_ratio: 0.15,
            level_tolerance: 2.0,
            starting_balance: 50000.0,
            max_daily_losses: 3,
            daily_loss_limit: 50.0,
            point_value: 20.0,
            slippage: 0.25,
            commission: 4.50,
        };

        // Create trader with state machine for real-time breakout detection
        let sm_config = StateMachineConfig::default();
        let mut trader = LiveTrader::new_with_state_machine(config, sm_config);

        // Set daily levels (already fetched before Databento connection)
        if let Some(levels) = daily_levels_result {
            trader.set_daily_levels(levels);
        }

        info!("State machine ready - will detect breakouts and extract LVNs in real-time");

        Some(Arc::new(RwLock::new(trader)))
    } else {
        None
    };

    // Shared bar aggregator state for trading
    let bar_aggregator = if trading_enabled {
        Some(Arc::new(RwLock::new(BarAggregator::new())))
    } else {
        None
    };

    // Create trade logger for CSV tracking
    let trade_logger = if trading_enabled {
        match TradeLogger::new("trades.csv") {
            Ok(logger) => {
                info!("Trade logger initialized - logging to trades.csv");
                Some(Arc::new(RwLock::new(logger)))
            }
            Err(e) => {
                warn!("Failed to create trade logger: {} - trades will not be logged to CSV", e);
                None
            }
        }
    } else {
        None
    };

    // Clone for trade processing
    let trader_clone = trader.clone();
    let bar_agg_clone = bar_aggregator.clone();
    let trade_logger_clone = trade_logger.clone();
    let tx_trading = state.tx.clone();

    // Process incoming records
    while let Some(record) = client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            let min_size = *state.min_size.read().await;

            // Determine buy/sell from aggressor side
            // 'A' = Ask side (buyer aggressor), 'B' = Bid side (seller aggressor)
            let side = match trade.side as u8 {
                b'A' | b'a' => "buy",
                b'B' | b'b' => "sell",
                _ => "buy", // Default
            };

            // Get symbol from instrument ID
            let symbol = get_symbol_from_record(&record, &symbols);
            let price = trade.price as f64 / 1_000_000_000.0; // Fixed-point conversion
            let timestamp_nanos = trade.hd.ts_event;
            let timestamp_millis = timestamp_nanos / 1_000_000;

            // Process trade through LVN trading strategy (if enabled)
            if let (Some(ref trader_arc), Some(ref bar_agg_arc)) = (&trader_clone, &bar_agg_clone) {
                let trading_side = if side == "buy" { Side::Buy } else { Side::Sell };
                let ts = DateTime::<Utc>::from_timestamp_nanos(timestamp_nanos as i64);

                let trading_trade = TradingTrade {
                    ts_event: ts,
                    price,
                    size: trade.size as u64,
                    side: trading_side,
                    symbol: symbol.clone(),
                };

                // Feed trade to state machine (for impulse profiling)
                {
                    let mut trader = trader_arc.write().await;
                    trader.process_trade(&trading_trade);
                }

                // Aggregate into bars and check for signals
                let mut bar_agg = bar_agg_arc.write().await;
                if let Some(completed_bar) = bar_agg.process_trade(ts, price, trade.size as u64, side == "buy", &symbol) {
                    // Process completed bar through trader
                    let mut trader = trader_arc.write().await;
                    if let Some(action) = trader.process_bar(&completed_bar) {
                        // Log trade action to CSV
                        if let Some(ref logger_arc) = trade_logger_clone {
                            let mut logger = logger_arc.write().await;
                            match &action {
                                TradeAction::Enter { direction, price, stop, target, .. } => {
                                    logger.log_entry(ts, direction, *price, *stop, *target);
                                }
                                TradeAction::Exit { direction, price, pnl_points, reason } => {
                                    logger.log_exit(ts, direction, *price, *pnl_points, reason);
                                }
                                TradeAction::UpdateStop { new_stop } => {
                                    logger.log_stop_update(ts, *new_stop);
                                }
                                TradeAction::FlattenAll { reason } => {
                                    logger.log_flatten(ts, reason);
                                }
                                TradeAction::SignalPending => {}
                            }
                        }

                        // Convert TradeAction to TradingSignal and broadcast
                        if let Some(signal) = trade_action_to_signal(&action, timestamp_millis, completed_bar.close) {
                            info!("Trading signal: {:?}", signal.signal_type);
                            let _ = tx_trading.send(WsMessage::TradingSignal(signal));
                        }
                    }
                }
            }

            // Only add to processing buffer if size meets minimum
            if trade.size >= min_size {
                let trade_msg = Trade {
                    symbol,
                    price,
                    size: trade.size,
                    side: side.to_string(),
                    timestamp: timestamp_millis,
                };

                // Add trade to processing buffer
                let mut pstate = processing_state.write().await;
                pstate.add_trade(trade_msg);
            }
        }
    }

    warn!("Databento stream ended");
    Ok(())
}

fn get_symbol_from_record(_record: &dyn Record, symbols: &[String]) -> String {
    // For simplicity, if we only have one symbol, return it
    // In production, you'd map instrument_id to symbol
    if symbols.len() == 1 {
        return symbols[0].clone();
    }

    // Default to first symbol - proper implementation would use symbol mapping
    symbols
        .first()
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

/// Real-time bar aggregator for trading
struct BarAggregator {
    current_bar: Option<BarBuilder>,
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

impl BarAggregator {
    fn new() -> Self {
        Self { current_bar: None }
    }

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

/// Convert a TradeAction to a TradingSignal for WebSocket broadcast
fn trade_action_to_signal(action: &TradeAction, timestamp: u64, current_price: f64) -> Option<TradingSignal> {
    match action {
        TradeAction::Enter { direction, price, stop, target, .. } => {
            Some(TradingSignal {
                timestamp,
                signal_type: "entry".to_string(),
                direction: match direction {
                    Direction::Long => "long".to_string(),
                    Direction::Short => "short".to_string(),
                },
                price: *price,
                stop: Some(*stop),
                target: Some(*target),
                pnl_points: None,
                reason: None,
                x: current_price,
            })
        }
        TradeAction::Exit { direction, price, pnl_points, reason } => {
            Some(TradingSignal {
                timestamp,
                signal_type: "exit".to_string(),
                direction: match direction {
                    Direction::Long => "long".to_string(),
                    Direction::Short => "short".to_string(),
                },
                price: *price,
                stop: None,
                target: None,
                pnl_points: Some(*pnl_points),
                reason: Some(reason.clone()),
                x: current_price,
            })
        }
        TradeAction::UpdateStop { new_stop } => {
            Some(TradingSignal {
                timestamp,
                signal_type: "stop_update".to_string(),
                direction: "".to_string(),
                price: *new_stop,
                stop: Some(*new_stop),
                target: None,
                pnl_points: None,
                reason: None,
                x: current_price,
            })
        }
        TradeAction::FlattenAll { reason } => {
            Some(TradingSignal {
                timestamp,
                signal_type: "flatten".to_string(),
                direction: "".to_string(),
                price: current_price,
                stop: None,
                target: None,
                pnl_points: None,
                reason: Some(reason.clone()),
                x: current_price,
            })
        }
        TradeAction::SignalPending => {
            // Signal pending doesn't need to be broadcast
            None
        }
    }
}
