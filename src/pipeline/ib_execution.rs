//! Interactive Brokers Order Execution
//!
//! Provides order management and execution via IB TWS or Gateway.
//! Used by databento_ib_live.rs for live trading.

use anyhow::{Result, Context};
use chrono::{DateTime, Utc, Timelike};
use std::sync::Arc;
use tracing::{info, warn, error, debug};

use ibapi::Client;
use ibapi::contracts::{Contract, SecurityType};
use ibapi::orders::{Action, order_builder};

use crate::bars::Bar;
use super::lvn_retest::Direction;
use super::trader::{LiveConfig, LiveTrader, TradeAction};

/// IB-specific configuration
#[derive(Debug, Clone)]
pub struct IbConfig {
    /// TWS/Gateway host (default: 127.0.0.1)
    pub host: String,
    /// TWS/Gateway port (paper: 7497, live: 7496)
    pub port: u16,
    /// Client ID (must be unique per connection)
    pub client_id: i32,
}

impl Default for IbConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 7497, // Paper trading port
            client_id: 1,
        }
    }
}

/// Create NQ futures contract for IB with default symbol
pub fn create_nq_contract() -> Contract {
    create_futures_contract("NQH6")
}

/// Create futures contract for IB with specified contract symbol
/// Supports both E-mini (NQ) and Micro (MNQ) contracts
/// Symbol format: [M]NQ + month code + year digit (e.g., NQH6, MNQH6 = March 2026)
/// Month codes: H=Mar, M=Jun, U=Sep, Z=Dec
pub fn create_futures_contract(local_symbol: &str) -> Contract {
    // Detect if micro contract
    let is_micro = local_symbol.starts_with("MNQ") || local_symbol.starts_with("MES")
                || local_symbol.starts_with("M2K") || local_symbol.starts_with("MYM");

    // Extract base symbol (NQ, ES, etc.)
    let base_symbol = if is_micro {
        &local_symbol[1..3]  // MNQ -> NQ
    } else {
        &local_symbol[0..2]  // NQH6 -> NQ
    };

    Contract {
        symbol: if is_micro { format!("M{}", base_symbol) } else { base_symbol.to_string() },
        security_type: SecurityType::Future,
        exchange: "CME".to_string(),
        currency: "USD".to_string(),
        local_symbol: local_symbol.to_string(),
        primary_exchange: "CME".to_string(),
        ..Default::default()
    }
}

/// Alias for backwards compatibility
pub fn create_nq_contract_with_symbol(local_symbol: &str) -> Contract {
    create_futures_contract(local_symbol)
}

/// Create a stock contract for testing (works with delayed data)
fn create_stock_contract(symbol: &str) -> Contract {
    Contract {
        symbol: symbol.to_string(),
        security_type: SecurityType::Stock,
        exchange: "SMART".to_string(),
        currency: "USD".to_string(),
        ..Default::default()
    }
}

/// Order Manager for IB - handles bracket order submission
pub struct IbOrderManager {
    client: Arc<Client>,
    contract: Contract,
    next_order_id: i32,
    current_parent_id: Option<i32>,
    current_stop_id: Option<i32>,
    current_profit_id: Option<i32>,
}

impl IbOrderManager {
    pub fn new(client: Arc<Client>, contract: Contract) -> Self {
        Self {
            client,
            contract,
            next_order_id: 1,
            current_parent_id: None,
            current_stop_id: None,
            current_profit_id: None,
        }
    }

    fn get_next_order_id(&mut self) -> i32 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Submit a bracket order (entry + stop + target)
    pub fn submit_bracket_order(
        &mut self,
        direction: Direction,
        contracts: i32,
        stop_price: f64,
        target_price: f64,
    ) -> Result<()> {
        let action = match direction {
            Direction::Long => Action::Buy,
            Direction::Short => Action::Sell,
        };

        let reverse_action = match direction {
            Direction::Long => Action::Sell,
            Direction::Short => Action::Buy,
        };

        // Parent order - market order to enter
        let parent_id = self.get_next_order_id();
        let mut parent = order_builder::market_order(action.clone(), contracts as f64);
        parent.order_id = parent_id;
        parent.transmit = false; // Don't transmit until children are attached

        // Stop loss order
        let stop_id = self.get_next_order_id();
        let mut stop_order = order_builder::stop(reverse_action.clone(), contracts as f64, stop_price);
        stop_order.order_id = stop_id;
        stop_order.parent_id = parent_id;
        stop_order.transmit = false;

        // Take profit order
        let profit_id = self.get_next_order_id();
        let mut profit_order = order_builder::limit_order(reverse_action, contracts as f64, target_price);
        profit_order.order_id = profit_id;
        profit_order.parent_id = parent_id;
        profit_order.transmit = true; // Transmit all orders now

        info!(
            "Submitting bracket: {} {} @ MKT | Stop: {:.2} | Target: {:.2}",
            if matches!(action, Action::Buy) { "BUY" } else { "SELL" },
            contracts,
            stop_price,
            target_price
        );

        // Place all three orders
        self.client.place_order(parent_id, &self.contract, &parent)?;
        self.client.place_order(stop_id, &self.contract, &stop_order)?;
        self.client.place_order(profit_id, &self.contract, &profit_order)?;

        self.current_parent_id = Some(parent_id);
        self.current_stop_id = Some(stop_id);
        self.current_profit_id = Some(profit_id);

        Ok(())
    }

    /// Modify the stop loss price
    pub fn modify_stop(&mut self, new_stop_price: f64, contracts: i32, direction: Direction) -> Result<()> {
        let Some(stop_id) = self.current_stop_id else {
            debug!("No stop order to modify");
            return Ok(());
        };

        let reverse_action = match direction {
            Direction::Long => Action::Sell,
            Direction::Short => Action::Buy,
        };

        let mut stop_order = order_builder::stop(reverse_action, contracts as f64, new_stop_price);
        stop_order.order_id = stop_id;
        if let Some(parent_id) = self.current_parent_id {
            stop_order.parent_id = parent_id;
        }
        stop_order.transmit = true;

        debug!("Modifying stop to {:.2}", new_stop_price);
        self.client.place_order(stop_id, &self.contract, &stop_order)?;

        Ok(())
    }

    /// Cancel all orders and flatten position
    pub fn flatten_all(&mut self) -> Result<()> {
        info!("Flattening all positions...");

        // Cancel child orders first
        if let Some(stop_id) = self.current_stop_id.take() {
            let _ = self.client.cancel_order(stop_id, "");
        }
        if let Some(profit_id) = self.current_profit_id.take() {
            let _ = self.client.cancel_order(profit_id, "");
        }

        // Cancel parent if still pending
        if let Some(parent_id) = self.current_parent_id.take() {
            let _ = self.client.cancel_order(parent_id, "");
        }

        info!("Flattened all positions");
        Ok(())
    }

    /// Clear order tracking
    pub fn clear_orders(&mut self) {
        self.current_parent_id = None;
        self.current_stop_id = None;
        self.current_profit_id = None;
    }
}

/// Main IB live trading loop
pub fn run_ib_live(config: LiveConfig, ib_config: IbConfig, paper_mode: bool) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("        LIVE TRADING - INTERACTIVE BROKERS                 ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");
    info!("Symbol: {} on {}", config.symbol, config.exchange);
    info!("Contracts: {}", config.contracts);
    info!("Mode: {}", if paper_mode { "PAPER" } else { "LIVE" });
    info!("Connecting to: {}:{}", ib_config.host, ib_config.port);
    info!("Trading Hours: {:02}:{:02} - {:02}:{:02} ET",
        config.start_hour,
        config.start_minute,
        config.end_hour,
        config.end_minute
    );
    info!("");

    // Initialize trader (same signal generation as Rithmic)
    let mut trader = LiveTrader::new(config.clone());

    // Load LVN levels
    info!("Loading LVN levels from {:?}...", config.cache_dir);
    let level_count = trader.load_lvn_levels(&config.cache_dir)?;
    info!("Loaded {} LVN levels", level_count);

    // Connect to IB
    let connection_url = format!("{}:{}", ib_config.host, ib_config.port);
    info!("Connecting to IB at {}...", connection_url);

    let client = Client::connect(&connection_url, ib_config.client_id)
        .context("Failed to connect to IB TWS/Gateway. Make sure TWS or IB Gateway is running.")?;

    let client = Arc::new(client);
    info!("Connected to IB");

    // Create contract and order manager
    let contract = create_nq_contract();
    let mut order_manager = IbOrderManager::new(client.clone(), contract.clone());

    // Track position state
    let mut current_entry_price: Option<f64> = None;
    let mut current_direction: Option<Direction> = None;

    // Subscribe to market data
    info!("Subscribing to market data for {:?}...", contract.symbol);

    // Request real-time bars (5-second bars, will aggregate to 1-second internally)
    let bars_subscription = client.realtime_bars(
        &contract,
        ibapi::market_data::realtime::BarSize::Sec5,
        ibapi::market_data::realtime::WhatToShow::Trades,
        false,
    )?;

    info!("");
    info!("═══════════════════════════════════════════════════════════");
    info!("                    TRADING STARTED                        ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");

    let mut last_status_time = std::time::Instant::now();

    // Main trading loop - process real-time bars
    for ib_bar in bars_subscription {
        // Convert IB bar to our bar format
        // Convert time::OffsetDateTime to chrono::DateTime<Utc>
        let timestamp = DateTime::from_timestamp(
            ib_bar.date.unix_timestamp(),
            ib_bar.date.nanosecond()
        ).unwrap_or_else(|| Utc::now());

        let bar = Bar {
            timestamp,
            open: ib_bar.open,
            high: ib_bar.high,
            low: ib_bar.low,
            close: ib_bar.close,
            volume: ib_bar.volume as u64,
            buy_volume: (ib_bar.volume as u64) / 2, // Approximate - IB doesn't provide buy/sell split
            sell_volume: (ib_bar.volume as u64) / 2,
            delta: 0, // Not available from IB bars directly
            trade_count: ib_bar.count as u64,
            symbol: config.symbol.clone(),
        };

        // Process bar through trader
        if let Some(action) = trader.process_bar(&bar) {
            match action {
                TradeAction::Enter { direction, price, stop, target, contracts } => {
                    current_entry_price = Some(price);
                    current_direction = Some(direction);

                    if let Err(e) = order_manager.submit_bracket_order(
                        direction,
                        contracts,
                        stop,
                        target,
                    ) {
                        error!("Failed to submit bracket order: {}", e);
                    }
                }
                TradeAction::Exit { direction: _, price: _, pnl_points, reason } => {
                    info!("Position closed: {} | P&L: {:.2} pts", reason, pnl_points);
                    order_manager.clear_orders();
                    current_entry_price = None;
                    current_direction = None;
                }
                TradeAction::UpdateStop { new_stop } => {
                    if let Some(dir) = current_direction {
                        if let Err(e) = order_manager.modify_stop(
                            new_stop,
                            config.contracts,
                            dir,
                        ) {
                            warn!("Failed to modify stop: {}", e);
                        }
                    }
                }
                TradeAction::FlattenAll { reason } => {
                    warn!("Flattening all: {}", reason);
                    if let Err(e) = order_manager.flatten_all() {
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

/// Run IB paper trading with demo credentials
pub fn run_ib_demo() -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("        IB DEMO CONNECTION TEST                            ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");
    info!("Make sure TWS/Gateway is running.");
    info!("");

    let ib_config = IbConfig::default();
    let connection_url = format!("{}:{}", ib_config.host, ib_config.port);

    let client = Client::connect(&connection_url, ib_config.client_id)
        .context("Failed to connect. Is TWS/Gateway running?")?;

    info!("✓ Connected to IB successfully!");

    // Get server time to verify connection
    let server_time = client.server_time()?;
    info!("✓ Server time: {}", server_time);

    // Create NQ contract and verify it's valid
    let contract = create_nq_contract();
    info!("✓ Created NQ futures contract: {:?}", contract.local_symbol);

    info!("");
    info!("Connection test successful! Ready for paper trading.");
    info!("");

    Ok(())
}

/// Test market data subscription with a stock symbol
pub fn run_ib_data_test(symbol: &str, duration_secs: u64) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("        IB MARKET DATA TEST                                ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");
    info!("Symbol: {}", symbol);
    info!("Duration: {} seconds", duration_secs);
    info!("");

    let ib_config = IbConfig::default();
    let connection_url = format!("{}:{}", ib_config.host, ib_config.port);

    let client = Client::connect(&connection_url, ib_config.client_id)
        .context("Failed to connect. Is TWS/Gateway running?")?;

    info!("✓ Connected to IB");

    // Create stock contract for testing
    let contract = create_stock_contract(symbol);
    info!("✓ Created contract for {}", symbol);

    // First verify the contract is valid
    info!("Verifying contract...");
    match client.contract_details(&contract) {
        Ok(details) => {
            let count = details.len();
            if count == 0 {
                warn!("Contract not found. Symbol {} may not be valid.", symbol);
            } else {
                info!("✓ Contract verified: {} matches found", count);
            }
        }
        Err(e) => {
            warn!("Could not verify contract: {}", e);
        }
    }

    // Try to get realtime bars
    info!("Subscribing to realtime bars...");

    let bars_result = client.realtime_bars(
        &contract,
        ibapi::market_data::realtime::BarSize::Sec5,
        ibapi::market_data::realtime::WhatToShow::Trades,
        false, // use regular trading hours only
    );

    match bars_result {
        Ok(bars_subscription) => {
            info!("✓ Subscription successful! Waiting for bars...");

            let start = std::time::Instant::now();
            let mut bar_count = 0;

            for bar in bars_subscription {
                bar_count += 1;
                info!(
                    "Bar #{}: O={:.2} H={:.2} L={:.2} C={:.2} V={} Count={}",
                    bar_count, bar.open, bar.high, bar.low, bar.close, bar.volume, bar.count
                );

                if start.elapsed().as_secs() >= duration_secs {
                    info!("Test duration reached.");
                    break;
                }
            }

            if bar_count == 0 {
                warn!("No bars received. Possible reasons:");
                warn!("  - Market is closed (it's Sunday or outside trading hours)");
                warn!("  - No market data subscription for this symbol");
                warn!("  - Contract specification issue");
            } else {
                info!("✓ Received {} bars in {} seconds", bar_count, start.elapsed().as_secs());
            }
        }
        Err(e) => {
            error!("Failed to subscribe to realtime bars: {}", e);
            error!("This usually means:");
            error!("  - No market data subscription");
            error!("  - Invalid contract specification");
            error!("  - Market data permissions issue");
        }
    }

    info!("");
    info!("Data test complete.");
    Ok(())
}

/// Test NQ futures contract specifically
pub fn run_ib_futures_test(duration_secs: u64) -> Result<()> {
    info!("═══════════════════════════════════════════════════════════");
    info!("        IB FUTURES DATA TEST                               ");
    info!("═══════════════════════════════════════════════════════════");
    info!("");

    let ib_config = IbConfig::default();
    let connection_url = format!("{}:{}", ib_config.host, ib_config.port);

    let client = Client::connect(&connection_url, ib_config.client_id)
        .context("Failed to connect. Is TWS/Gateway running?")?;

    info!("✓ Connected to IB");

    // Create NQ contract
    let contract = create_nq_contract();
    info!("Contract: {} ({})", contract.symbol, contract.local_symbol);

    // First verify the contract is valid
    info!("Verifying contract...");
    match client.contract_details(&contract) {
        Ok(details) => {
            let mut count = 0;
            for detail in details {
                count += 1;
                info!("  Contract Details #{}: {} - {} ({})",
                    count,
                    detail.contract.local_symbol,
                    detail.long_name,
                    detail.contract.exchange
                );
            }
            if count == 0 {
                error!("NQ contract not found!");
                error!("  - Check that NQH6 is the correct contract month");
                error!("  - Try NQM6 (June) or NQU6 (September) if H6 has expired");
                return Ok(());
            }
            info!("✓ Found {} contract(s)", count);
        }
        Err(e) => {
            error!("Could not verify contract: {}", e);
            error!("This likely means no CME data subscription.");
            return Ok(());
        }
    }

    // Try historical data first (might work with delayed data)
    info!("Trying historical data...");
    use ibapi::market_data::historical::{BarSize as HistBarSize, ToDuration, WhatToShow as HistWhatToShow};

    let hist_result = client.historical_data(
        &contract,
        None, // end time (None = now)
        300_i32.seconds(), // 5 minutes = 300 seconds
        HistBarSize::Sec5,
        HistWhatToShow::Trades,
        true, // use RTH
    );

    match hist_result {
        Ok(bars) => {
            info!("✓ Received {} historical bars", bars.bars.len());
            for (i, bar) in bars.bars.iter().take(5).enumerate() {
                info!("  Bar #{}: {} | O={:.2} H={:.2} L={:.2} C={:.2} | V={}",
                    i + 1,
                    bar.date,
                    bar.open, bar.high, bar.low, bar.close,
                    bar.volume
                );
            }
            if bars.bars.len() > 5 {
                info!("  ... and {} more bars", bars.bars.len() - 5);
            }
        }
        Err(e) => {
            warn!("Historical data failed: {}", e);
            warn!("This confirms you need CME market data subscription.");
        }
    }

    // Try to get realtime bars
    info!("");
    info!("Trying realtime bars (duration: {} sec)...", duration_secs);

    let bars_result = client.realtime_bars(
        &contract,
        ibapi::market_data::realtime::BarSize::Sec5,
        ibapi::market_data::realtime::WhatToShow::Trades,
        false,
    );

    match bars_result {
        Ok(bars_subscription) => {
            info!("✓ Subscription started. Waiting for bars...");

            let start = std::time::Instant::now();
            let mut bar_count = 0;

            for bar in bars_subscription {
                bar_count += 1;
                info!(
                    "Bar #{}: {} | O={:.2} H={:.2} L={:.2} C={:.2} | V={}",
                    bar_count,
                    bar.date,
                    bar.open, bar.high, bar.low, bar.close,
                    bar.volume
                );

                if start.elapsed().as_secs() >= duration_secs {
                    info!("Test duration reached.");
                    break;
                }
            }

            if bar_count == 0 {
                warn!("No realtime bars received.");
                warn!("");
                warn!("To get CME futures data, you need to subscribe in IB:");
                warn!("  1. Log into IB Account Management");
                warn!("  2. Go to Settings > User Settings > Market Data Subscriptions");
                warn!("  3. Subscribe to 'CME Real-Time' (~$15/month for non-pro)");
            } else {
                info!("✓ Received {} bars", bar_count);
            }
        }
        Err(e) => {
            error!("Failed to subscribe: {}", e);
        }
    }

    Ok(())
}
