//! Live Trading with Databento Data + Interactive Brokers Execution
//!
//! Uses Databento's live streaming API for real-time tick data with accurate
//! buy/sell attribution (delta), and IB for order execution.
//!
//! This is the production live trading module.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use databento::{
    dbn::{Record, Schema, SType, TradeMsg},
    live::Subscription,
    LiveClient,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

use ibapi::Client as IbClient;

use crate::bars::Bar;
use super::ib_live::{IbConfig, IbOrderManager, create_nq_contract};
use super::lvn_retest::Direction;
use super::live_trader::{LiveConfig, LiveTrader, TradeAction};

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

    /// Flush any remaining bar data
    fn flush(&mut self) -> Option<Bar> {
        self.current_bar.take().map(|bar| bar.to_bar(&self.symbol))
    }
}

/// Run live trading with Databento data and IB execution
pub async fn run_databento_ib_live(
    api_key: String,
    config: LiveConfig,
    ib_config: IbConfig,
    paper_mode: bool,
) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("     LIVE TRADING - DATABENTO DATA + IB EXECUTION          ");
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

    // Initialize trader (signal generation)
    let mut trader = LiveTrader::new(config.clone());

    // Load LVN levels
    info!("Loading LVN levels from {:?}...", config.cache_dir);
    let level_count = trader.load_lvn_levels(&config.cache_dir)?;
    info!("Loaded {} LVN levels", level_count);

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
