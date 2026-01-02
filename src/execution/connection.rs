//! Rithmic API connection management with auto-reconnect

use anyhow::{Result, bail};
use tokio::sync::mpsc;
use tracing::{info, warn, debug};
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;

use rithmic_rs::{
    RithmicConfig, RithmicEnv, ConnectStrategy,
    RithmicOrderPlant, RithmicOrderPlantHandle,
    api::rithmic_command_types::RithmicBracketOrder,
    rti::messages::RithmicMessage,
    ws::RithmicStream,
};

use super::config::{ExecutionConfig, ExecutionMode};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

/// Events from the Rithmic connection
#[derive(Debug, Clone)]
pub enum RithmicEvent {
    /// Connection established
    Connected,
    /// Connection lost
    Disconnected { reason: String },
    /// Order acknowledged
    OrderAcknowledged { order_id: String, exchange_order_id: String },
    /// Order filled
    OrderFilled { order_id: String, fill_price: f64, fill_quantity: i32 },
    /// Order rejected
    OrderRejected { order_id: String, reason: String },
    /// Order cancelled
    OrderCancelled { order_id: String },
    /// Account update
    AccountUpdate { balance: f64, open_pnl: f64 },
    /// Error
    Error { message: String },
}

/// Active bracket order tracking
#[derive(Debug, Clone)]
pub struct ActiveOrder {
    pub local_id: String,
    pub basket_id: Option<String>,
    pub symbol: String,
    pub exchange: String,
    pub side: String,
    pub quantity: i32,
    pub entry_price: Option<f64>,
    pub stop_price: f64,
    pub target_price: f64,
}

/// Rithmic connection wrapper with auto-reconnect
pub struct RithmicConnection {
    config: ExecutionConfig,
    state: ConnectionState,
    event_tx: mpsc::Sender<RithmicEvent>,
    event_rx: mpsc::Receiver<RithmicEvent>,
    reconnect_attempts: u32,
    max_reconnect_attempts: u32,

    // Rithmic order plant handle (only present when connected in Paper/Live mode)
    order_plant: Option<RithmicOrderPlant>,
    order_handle: Option<Arc<Mutex<RithmicOrderPlantHandle>>>,

    // Active orders for tracking
    active_orders: Vec<ActiveOrder>,
    order_counter: u64,
}

impl RithmicConnection {
    /// Create a new Rithmic connection
    pub fn new(config: ExecutionConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel(1000);

        Self {
            config,
            state: ConnectionState::Disconnected,
            event_tx,
            event_rx,
            reconnect_attempts: 0,
            max_reconnect_attempts: 5,
            order_plant: None,
            order_handle: None,
            active_orders: Vec::new(),
            order_counter: 0,
        }
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Generate a unique local order ID
    fn next_order_id(&mut self) -> String {
        self.order_counter += 1;
        format!("HTB_{}", self.order_counter)
    }

    /// Connect to Rithmic
    pub async fn connect(&mut self) -> Result<()> {
        if self.config.mode == ExecutionMode::Simulation {
            info!("Simulation mode - skipping Rithmic connection");
            self.state = ConnectionState::Connected;
            return Ok(());
        }

        self.state = ConnectionState::Connecting;
        info!("Connecting to Rithmic {} environment...", self.config.rithmic_env);

        // Validate credentials
        if self.config.rithmic_user.is_empty() {
            bail!("RITHMIC_USER not configured");
        }
        if self.config.rithmic_password.is_empty() {
            bail!("RITHMIC_PASSWORD not configured");
        }

        // Build rithmic-rs config
        let rithmic_env = match self.config.mode {
            ExecutionMode::Paper => RithmicEnv::Demo,
            ExecutionMode::Live => RithmicEnv::Live,
            ExecutionMode::Simulation => unreachable!(),
        };

        // Get URLs from environment or use defaults
        let url_prefix = if self.config.mode == ExecutionMode::Paper { "RITHMIC_DEMO" } else { "RITHMIC_LIVE" };
        let url = std::env::var(format!("{}_URL", url_prefix))
            .unwrap_or_else(|_| "wss://rituz00100.rithmic.com:443".to_string());
        let beta_url = std::env::var(format!("{}_ALT_URL", url_prefix))
            .unwrap_or_else(|_| url.clone());

        let rithmic_config = RithmicConfig::builder(rithmic_env)
            .account_id(&self.config.rithmic_account_id)
            .fcm_id(&self.config.rithmic_fcm_id)
            .ib_id(&self.config.rithmic_ib_id)
            .user(&self.config.rithmic_user)
            .password(&self.config.rithmic_password)
            .url(&url)
            .beta_url(&beta_url)
            .system_name(&self.config.rithmic_system)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Rithmic config: {}", e))?;

        // Connect to order plant
        info!("Connecting to Rithmic Order Plant at {}...", url);
        let order_plant = RithmicOrderPlant::connect(&rithmic_config, ConnectStrategy::Simple)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Rithmic: {}", e))?;

        let handle: RithmicOrderPlantHandle = order_plant.get_handle();

        // Login
        info!("Logging in to Rithmic Order Plant...");
        let _login_resp = handle.login().await
            .map_err(|e| anyhow::anyhow!("Rithmic login failed: {}", e))?;

        // Subscribe to order and bracket updates
        info!("Subscribing to order updates...");
        let _order_resp = handle.subscribe_order_updates().await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to order updates: {}", e))?;

        let _bracket_resp = handle.subscribe_bracket_updates().await
            .map_err(|e| anyhow::anyhow!("Failed to subscribe to bracket updates: {}", e))?;

        self.order_plant = Some(order_plant);
        self.order_handle = Some(Arc::new(Mutex::new(handle)));
        self.state = ConnectionState::Connected;

        info!("Successfully connected to Rithmic Order Plant");
        let _ = self.event_tx.send(RithmicEvent::Connected).await;

        Ok(())
    }

    /// Disconnect from Rithmic
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.config.mode == ExecutionMode::Simulation {
            self.state = ConnectionState::Disconnected;
            return Ok(());
        }

        info!("Disconnecting from Rithmic...");

        if let Some(handle) = &self.order_handle {
            let handle = handle.lock().await;
            if let Err(e) = handle.disconnect().await {
                warn!("Error during Rithmic disconnect: {}", e);
            }
        }

        self.order_handle = None;
        self.order_plant = None;
        self.state = ConnectionState::Disconnected;

        let _ = self.event_tx.send(RithmicEvent::Disconnected {
            reason: "User requested disconnect".to_string(),
        }).await;

        Ok(())
    }

    /// Attempt to reconnect
    pub async fn reconnect(&mut self) -> Result<()> {
        if self.reconnect_attempts >= self.max_reconnect_attempts {
            self.state = ConnectionState::Failed;
            bail!("Max reconnect attempts ({}) exceeded", self.max_reconnect_attempts);
        }

        self.state = ConnectionState::Reconnecting;
        self.reconnect_attempts += 1;

        let delay = Duration::from_secs(2u64.pow(self.reconnect_attempts));
        warn!(
            "Reconnecting to Rithmic (attempt {}/{}) in {:?}...",
            self.reconnect_attempts,
            self.max_reconnect_attempts,
            delay
        );

        tokio::time::sleep(delay).await;
        self.connect().await
    }

    /// Reset reconnect counter (call after successful operation)
    pub fn reset_reconnect_counter(&mut self) {
        self.reconnect_attempts = 0;
    }

    /// Submit a bracket order (entry with stop and target)
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol (e.g., "MNQH6" for Micro NQ March 2026)
    /// * `exchange` - Exchange code (e.g., "CME")
    /// * `side` - "BUY" or "SELL"
    /// * `quantity` - Number of contracts
    /// * `stop_ticks` - Stop loss distance in ticks
    /// * `profit_ticks` - Take profit distance in ticks
    /// * `entry_price` - Optional limit price (None for market order)
    pub async fn submit_bracket_order(
        &mut self,
        symbol: &str,
        exchange: &str,
        side: &str,
        quantity: i32,
        stop_ticks: i32,
        profit_ticks: i32,
        entry_price: Option<f64>,
    ) -> Result<String> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        let local_id = self.next_order_id();

        if self.config.mode == ExecutionMode::Simulation {
            debug!(
                "SIMULATION: Bracket {} {} {} @ {:?} stop={} ticks, target={} ticks",
                side, quantity, symbol, entry_price, stop_ticks, profit_ticks
            );

            self.active_orders.push(ActiveOrder {
                local_id: local_id.clone(),
                basket_id: None,
                symbol: symbol.to_string(),
                exchange: exchange.to_string(),
                side: side.to_string(),
                quantity,
                entry_price,
                stop_price: 0.0, // Calculated after fill
                target_price: 0.0,
            });

            return Ok(local_id);
        }

        let handle = self.order_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Order handle not available"))?;

        // action: 1 = Buy, 2 = Sell
        let action = if side.to_uppercase() == "BUY" { 1 } else { 2 };

        // ordertype: 1 = Limit, 2 = Market
        let ordertype = if entry_price.is_some() { 1 } else { 2 };

        let bracket_order = RithmicBracketOrder {
            action,
            duration: 2, // Day order
            exchange: exchange.to_string(),
            localid: local_id.clone(),
            ordertype,
            price: entry_price,
            profit_ticks,
            qty: quantity,
            stop_ticks,
            symbol: symbol.to_string(),
        };

        info!(
            "Placing bracket order: {} {} {} @ {:?} stop={} ticks, target={} ticks",
            side, quantity, symbol, entry_price, stop_ticks, profit_ticks
        );

        let handle = handle.lock().await;
        let responses = handle.place_bracket_order(bracket_order).await
            .map_err(|e| anyhow::anyhow!("Failed to place bracket order: {}", e))?;

        // Extract basket_id from response if available
        let basket_id = responses.first().and_then(|r| {
            if let RithmicMessage::RithmicOrderNotification(ref notif) = r.message {
                notif.basket_id.clone()
            } else {
                None
            }
        });

        self.active_orders.push(ActiveOrder {
            local_id: local_id.clone(),
            basket_id,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            side: side.to_string(),
            quantity,
            entry_price,
            stop_price: 0.0,
            target_price: 0.0,
        });

        info!("Bracket order placed: local_id={}", local_id);
        Ok(local_id)
    }

    /// Submit a market order
    pub async fn submit_market_order(
        &mut self,
        symbol: &str,
        side: &str,
        quantity: i32,
    ) -> Result<String> {
        // Use bracket order with 0 stop/profit ticks for pure market order
        // Or we could use place_new_order, but bracket is simpler for our use case
        self.submit_bracket_order(symbol, &self.config.exchange.clone(), side, quantity, 0, 0, None).await
    }

    /// Submit a stop order
    pub async fn submit_stop_order(
        &mut self,
        symbol: &str,
        side: &str,
        quantity: i32,
        stop_price: f64,
    ) -> Result<String> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            let order_id = self.next_order_id();
            debug!("SIMULATION: Stop {} {} {} @ {:.2}", side, quantity, symbol, stop_price);
            return Ok(order_id);
        }

        // For standalone stop orders, use place_new_order
        // This would require more complex implementation
        // For now, our strategy uses bracket orders primarily
        warn!("Standalone stop orders not yet implemented - use bracket orders");
        Ok(self.next_order_id())
    }

    /// Submit a limit order
    pub async fn submit_limit_order(
        &mut self,
        symbol: &str,
        side: &str,
        quantity: i32,
        limit_price: f64,
    ) -> Result<String> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            let order_id = self.next_order_id();
            debug!("SIMULATION: Limit {} {} {} @ {:.2}", side, quantity, symbol, limit_price);
            return Ok(order_id);
        }

        warn!("Standalone limit orders not yet implemented - use bracket orders");
        Ok(self.next_order_id())
    }

    /// Modify a bracket order's stop level
    ///
    /// # Arguments
    /// * `order_id` - Local order ID or basket ID
    /// * `new_stop_ticks` - New stop distance in ticks (relative adjustment)
    pub async fn modify_stop(
        &mut self,
        order_id: &str,
        new_stop_ticks: i32,
    ) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            debug!("SIMULATION: Modify stop {} to {} ticks", order_id, new_stop_ticks);
            return Ok(());
        }

        let handle = self.order_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Order handle not available"))?;

        // Find the basket_id for this order
        let basket_id = self.active_orders.iter()
            .find(|o| o.local_id == order_id || o.basket_id.as_deref() == Some(order_id))
            .and_then(|o| o.basket_id.clone())
            .unwrap_or_else(|| order_id.to_string());

        info!("Modifying stop for order {} to {} ticks", basket_id, new_stop_ticks);

        let handle = handle.lock().await;
        handle.adjust_stop(&basket_id, new_stop_ticks).await
            .map_err(|e| anyhow::anyhow!("Failed to modify stop: {}", e))?;

        Ok(())
    }

    /// Modify an existing order (for trailing stop updates)
    pub async fn modify_order(
        &mut self,
        order_id: &str,
        new_stop_price: Option<f64>,
        new_limit_price: Option<f64>,
    ) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            debug!(
                "SIMULATION: Modify order {} stop={:?} limit={:?}",
                order_id, new_stop_price, new_limit_price
            );
            return Ok(());
        }

        // For now, we only support stop modification via adjust_stop
        // Full order modification would require calculating tick difference
        if new_stop_price.is_some() {
            warn!("Price-based stop modification not yet implemented - use modify_stop with ticks");
        }

        Ok(())
    }

    /// Cancel an order
    pub async fn cancel_order(&mut self, order_id: &str) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            debug!("SIMULATION: Cancel order {}", order_id);
            self.active_orders.retain(|o| o.local_id != order_id);
            return Ok(());
        }

        let handle = self.order_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Order handle not available"))?;

        let handle = handle.lock().await;
        handle.cancel_order(rithmic_rs::api::rithmic_command_types::RithmicCancelOrder {
            id: order_id.to_string(),
        }).await.map_err(|e| anyhow::anyhow!("Failed to cancel order: {}", e))?;

        self.active_orders.retain(|o| o.local_id != order_id && o.basket_id.as_deref() != Some(order_id));

        Ok(())
    }

    /// Cancel all open orders (flatten)
    pub async fn cancel_all_orders(&mut self) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        info!("Cancelling all open orders...");

        if self.config.mode == ExecutionMode::Simulation {
            debug!("SIMULATION: Cancel all orders");
            self.active_orders.clear();
            return Ok(());
        }

        let handle = self.order_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Order handle not available"))?;

        let handle = handle.lock().await;
        handle.cancel_all_orders().await
            .map_err(|e| anyhow::anyhow!("Failed to cancel all orders: {}", e))?;

        self.active_orders.clear();

        Ok(())
    }

    /// Exit all positions for a symbol
    pub async fn exit_position(&mut self, symbol: &str, exchange: &str) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        info!("Exiting position for {} on {}...", symbol, exchange);

        if self.config.mode == ExecutionMode::Simulation {
            debug!("SIMULATION: Exit position {} {}", symbol, exchange);
            self.active_orders.retain(|o| o.symbol != symbol);
            return Ok(());
        }

        let handle = self.order_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Order handle not available"))?;

        let handle = handle.lock().await;
        handle.exit_position(symbol, exchange).await
            .map_err(|e| anyhow::anyhow!("Failed to exit position: {}", e))?;

        self.active_orders.retain(|o| o.symbol != symbol);

        Ok(())
    }

    /// Get next event from the connection
    pub async fn next_event(&mut self) -> Option<RithmicEvent> {
        self.event_rx.recv().await
    }

    /// Try to get event without blocking
    pub fn try_next_event(&mut self) -> Option<RithmicEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Get event sender for external use
    pub fn event_sender(&self) -> mpsc::Sender<RithmicEvent> {
        self.event_tx.clone()
    }

    /// Get list of active orders
    pub fn active_orders(&self) -> &[ActiveOrder] {
        &self.active_orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulation_connection() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            ..Default::default()
        };

        let mut conn = RithmicConnection::new(config);
        conn.connect().await.unwrap();

        assert!(conn.is_connected());
    }

    #[tokio::test]
    async fn test_simulation_bracket_order() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            ..Default::default()
        };

        let mut conn = RithmicConnection::new(config);
        conn.connect().await.unwrap();

        // Test bracket order
        let order_id = conn.submit_bracket_order(
            "MNQH6",
            "CME",
            "BUY",
            1,
            4, // 4 ticks stop
            8, // 8 ticks profit
            None, // market entry
        ).await.unwrap();

        assert!(!order_id.is_empty());
        assert_eq!(conn.active_orders().len(), 1);
    }

    #[tokio::test]
    async fn test_simulation_cancel_all() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            ..Default::default()
        };

        let mut conn = RithmicConnection::new(config);
        conn.connect().await.unwrap();

        // Place some orders
        conn.submit_bracket_order("MNQH6", "CME", "BUY", 1, 4, 8, None).await.unwrap();
        conn.submit_bracket_order("MNQH6", "CME", "SELL", 1, 4, 8, None).await.unwrap();

        assert_eq!(conn.active_orders().len(), 2);

        // Cancel all
        conn.cancel_all_orders().await.unwrap();
        assert_eq!(conn.active_orders().len(), 0);
    }
}
