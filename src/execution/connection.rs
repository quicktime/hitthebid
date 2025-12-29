//! Rithmic API connection management with auto-reconnect

use anyhow::{Result, bail};
use tokio::sync::mpsc;
use tracing::{info, warn, debug};
use std::time::Duration;

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

/// Rithmic connection wrapper with auto-reconnect
pub struct RithmicConnection {
    config: ExecutionConfig,
    state: ConnectionState,
    event_tx: mpsc::Sender<RithmicEvent>,
    event_rx: mpsc::Receiver<RithmicEvent>,
    reconnect_attempts: u32,
    max_reconnect_attempts: u32,
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

        // TODO: Implement actual rithmic-rs connection
        // For now, this is a placeholder that will be filled in when
        // we have actual Rithmic credentials to test with
        //
        // The rithmic-rs crate provides:
        // - RithmicClient for order execution
        // - Async WebSocket connection
        // - Protobuf message handling
        //
        // Example (to be implemented):
        // let client = rithmic_rs::RithmicClient::new(
        //     &self.config.rithmic_user,
        //     &password,
        //     &self.config.rithmic_system,
        // ).await?;
        //
        // client.login().await?;
        // client.subscribe_order_updates().await?;

        warn!("Rithmic connection not yet implemented - running in dry-run mode");
        self.state = ConnectionState::Connected;

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

        // TODO: Clean disconnect
        // client.logout().await?;

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

    /// Submit a market order
    pub async fn submit_market_order(
        &mut self,
        symbol: &str,
        side: &str,
        quantity: i32,
    ) -> Result<String> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            let order_id = uuid::Uuid::new_v4().to_string();
            debug!("SIMULATION: Market {} {} {} @ market", side, quantity, symbol);
            return Ok(order_id);
        }

        // TODO: Implement actual order submission
        // let order = rithmic_rs::Order::market(symbol, side, quantity);
        // let order_id = client.submit_order(order).await?;

        warn!("Order submission not yet implemented");
        Ok(uuid::Uuid::new_v4().to_string())
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
            let order_id = uuid::Uuid::new_v4().to_string();
            debug!("SIMULATION: Stop {} {} {} @ {:.2}", side, quantity, symbol, stop_price);
            return Ok(order_id);
        }

        // TODO: Implement actual stop order submission
        warn!("Stop order submission not yet implemented");
        Ok(uuid::Uuid::new_v4().to_string())
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
            let order_id = uuid::Uuid::new_v4().to_string();
            debug!("SIMULATION: Limit {} {} {} @ {:.2}", side, quantity, symbol, limit_price);
            return Ok(order_id);
        }

        // TODO: Implement actual limit order submission
        warn!("Limit order submission not yet implemented");
        Ok(uuid::Uuid::new_v4().to_string())
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

        // TODO: Implement order modification
        warn!("Order modification not yet implemented");
        Ok(())
    }

    /// Cancel an order
    pub async fn cancel_order(&mut self, order_id: &str) -> Result<()> {
        if !self.is_connected() {
            bail!("Not connected to Rithmic");
        }

        if self.config.mode == ExecutionMode::Simulation {
            debug!("SIMULATION: Cancel order {}", order_id);
            return Ok(());
        }

        // TODO: Implement order cancellation
        warn!("Order cancellation not yet implemented");
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
            return Ok(());
        }

        // TODO: Implement cancel all
        warn!("Cancel all orders not yet implemented");
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
    async fn test_simulation_orders() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            ..Default::default()
        };

        let mut conn = RithmicConnection::new(config);
        conn.connect().await.unwrap();

        let order_id = conn.submit_market_order("NQ.c.0", "BUY", 1).await.unwrap();
        assert!(!order_id.is_empty());

        let stop_id = conn.submit_stop_order("NQ.c.0", "SELL", 1, 21500.0).await.unwrap();
        assert!(!stop_id.is_empty());
    }
}
