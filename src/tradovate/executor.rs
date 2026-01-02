//! Tradovate Trade Executor
//!
//! Translates TradeAction signals to Tradovate API calls.
//! Follows the same pattern as TopstepExecutor and IbOrderManager.

use anyhow::Result;
use tracing::{debug, error, info, warn};

use super::client::TradovateClient;
use super::models::{Account, Contract, OrderAction};

/// Direction of a trade
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Long,
    Short,
}

/// Trade action from the trading system
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
    /// Update stop loss price
    UpdateStop { new_stop: f64 },
    /// Signal pending (waiting for next bar)
    SignalPending,
    /// Flatten all positions
    FlattenAll { reason: String },
}

/// Tracked position state
#[derive(Debug, Clone)]
struct ExecutorPosition {
    direction: Direction,
    contracts: i32,
    entry_price: f64,
    stop_order_id: Option<i64>,
    target_order_id: Option<i64>,
}

/// Tradovate trade executor
///
/// Manages order submission and position tracking for the Tradovate API.
pub struct TradovateExecutor {
    client: TradovateClient,
    account: Account,
    contract: Contract,
    symbol: String,
    tick_size: f64,
    position: Option<ExecutorPosition>,
}

impl TradovateExecutor {
    /// Create a new executor
    ///
    /// Authenticates with Tradovate and looks up the account and contract.
    pub async fn new(mut client: TradovateClient, symbol: &str) -> Result<Self> {
        // Ensure authenticated
        client.ensure_authenticated().await?;

        // Get account
        let account = client.get_first_account().await?;
        info!("Using Tradovate account: {} (ID: {})", account.name, account.id);

        // Find contract
        let contract = client.find_contract(symbol).await?;
        let tick_size = contract.price_increment;
        info!(
            "Using contract: {} (ID: {}) - tick size: {}",
            contract.name, contract.id, tick_size
        );

        Ok(Self {
            client,
            account,
            contract,
            symbol: symbol.to_string(),
            tick_size,
            position: None,
        })
    }

    /// Execute a trade action
    pub async fn execute(&mut self, action: TradeAction) -> Result<()> {
        // Ensure token is still valid
        self.client.ensure_authenticated().await?;

        match action {
            TradeAction::Enter {
                direction,
                price: _,
                stop,
                target,
                contracts,
            } => {
                self.submit_bracket_order(direction, contracts, stop, target)
                    .await
            }
            TradeAction::Exit { reason, .. } => {
                info!("Closing position: {}", reason);
                self.close_position().await
            }
            TradeAction::UpdateStop { new_stop } => self.modify_stop(new_stop).await,
            TradeAction::FlattenAll { reason } => {
                warn!("Flattening all positions: {}", reason);
                self.flatten_all().await
            }
            TradeAction::SignalPending => {
                // Nothing to do
                Ok(())
            }
        }
    }

    /// Submit a bracket order (market entry + stop loss + take profit)
    async fn submit_bracket_order(
        &mut self,
        direction: Direction,
        contracts: i32,
        stop_price: f64,
        target_price: f64,
    ) -> Result<()> {
        if self.position.is_some() {
            warn!("Already in position, ignoring new entry signal");
            return Ok(());
        }

        let entry_action = match direction {
            Direction::Long => OrderAction::Buy,
            Direction::Short => OrderAction::Sell,
        };

        info!(
            "Submitting bracket order: {} {} {} @ MKT | Stop: {:.2} | Target: {:.2}",
            entry_action,
            contracts,
            self.symbol,
            stop_price,
            target_price
        );

        // 1. Place market entry order
        let entry_order = self
            .client
            .place_market_order(&self.account, &self.symbol, entry_action, contracts)
            .await?;

        // Estimate entry price (will be updated when we get fill)
        let estimated_entry = (stop_price + target_price) / 2.0;

        // 2. Place stop loss order
        let stop_action = match direction {
            Direction::Long => OrderAction::Sell,
            Direction::Short => OrderAction::Buy,
        };

        let stop_order = self
            .client
            .place_stop_order(&self.account, &self.symbol, stop_action, contracts, stop_price)
            .await?;

        // 3. Place take profit order (limit)
        let target_order = self
            .client
            .place_limit_order(&self.account, &self.symbol, stop_action, contracts, target_price)
            .await?;

        // Update position state
        self.position = Some(ExecutorPosition {
            direction,
            contracts,
            entry_price: estimated_entry,
            stop_order_id: Some(stop_order.id),
            target_order_id: Some(target_order.id),
        });

        info!(
            "Bracket order submitted - Entry: {}, Stop: {}, Target: {}",
            entry_order.id, stop_order.id, target_order.id
        );

        Ok(())
    }

    /// Modify the stop loss price
    async fn modify_stop(&mut self, new_stop: f64) -> Result<()> {
        let pos = match &self.position {
            Some(p) => p.clone(),
            None => {
                debug!("No position to modify stop for");
                return Ok(());
            }
        };

        if let Some(stop_order_id) = pos.stop_order_id {
            debug!("Modifying stop order {} to {:.2}", stop_order_id, new_stop);

            match self
                .client
                .modify_order(stop_order_id, None, Some(new_stop))
                .await
            {
                Ok(_) => {
                    info!("Stop modified to {:.2}", new_stop);
                }
                Err(e) => {
                    warn!("Failed to modify stop: {}, placing new stop order", e);

                    // Cancel old stop and place new one
                    let _ = self.client.cancel_order(stop_order_id).await;

                    let stop_action = match pos.direction {
                        Direction::Long => OrderAction::Sell,
                        Direction::Short => OrderAction::Buy,
                    };

                    let new_order = self
                        .client
                        .place_stop_order(
                            &self.account,
                            &self.symbol,
                            stop_action,
                            pos.contracts,
                            new_stop,
                        )
                        .await?;

                    // Update position with new stop order ID
                    if let Some(ref mut p) = self.position {
                        p.stop_order_id = Some(new_order.id);
                    }

                    info!("New stop order placed: ID {} at {:.2}", new_order.id, new_stop);
                }
            }
        } else {
            warn!("No stop order to modify, placing new stop order");

            let stop_action = match pos.direction {
                Direction::Long => OrderAction::Sell,
                Direction::Short => OrderAction::Buy,
            };

            let new_order = self
                .client
                .place_stop_order(
                    &self.account,
                    &self.symbol,
                    stop_action,
                    pos.contracts,
                    new_stop,
                )
                .await?;

            if let Some(ref mut p) = self.position {
                p.stop_order_id = Some(new_order.id);
            }
        }

        Ok(())
    }

    /// Close the current position
    async fn close_position(&mut self) -> Result<()> {
        if self.position.is_none() {
            debug!("No position to close");
            return Ok(());
        }

        // Cancel any open orders first
        self.cancel_all_orders().await?;

        // Get current position from exchange
        if let Some(exchange_pos) = self
            .client
            .get_position_for_contract(self.account.id, self.contract.id)
            .await?
        {
            if exchange_pos.net_pos != 0 {
                self.client
                    .flatten_position(&self.account, &self.symbol, &exchange_pos)
                    .await?;
            }
        }

        self.position = None;
        info!("Position closed");
        Ok(())
    }

    /// Flatten all positions and cancel all orders
    async fn flatten_all(&mut self) -> Result<()> {
        // Cancel all open orders
        self.cancel_all_orders().await?;

        // Close any position
        if let Some(exchange_pos) = self
            .client
            .get_position_for_contract(self.account.id, self.contract.id)
            .await?
        {
            if exchange_pos.net_pos != 0 {
                if let Err(e) = self
                    .client
                    .flatten_position(&self.account, &self.symbol, &exchange_pos)
                    .await
                {
                    error!("Failed to flatten position: {}", e);
                }
            }
        }

        self.position = None;
        info!("All positions flattened");
        Ok(())
    }

    /// Cancel all open orders for this contract
    async fn cancel_all_orders(&mut self) -> Result<()> {
        let orders = self.client.get_working_orders(self.account.id).await?;

        for order in orders {
            if order.contract_id == self.contract.id {
                if let Err(e) = self.client.cancel_order(order.id).await {
                    warn!("Failed to cancel order {}: {}", order.id, e);
                } else {
                    debug!("Canceled order {}", order.id);
                }
            }
        }

        // Clear tracked order IDs
        if let Some(ref mut pos) = self.position {
            pos.stop_order_id = None;
            pos.target_order_id = None;
        }

        Ok(())
    }

    /// Check if currently in a position
    pub fn is_flat(&self) -> bool {
        self.position.is_none()
    }

    /// Get current position info
    pub fn position_info(&self) -> Option<(Direction, i32)> {
        self.position.as_ref().map(|p| (p.direction, p.contracts))
    }

    /// Sync position state with exchange
    pub async fn sync_position(&mut self) -> Result<()> {
        let exchange_pos = self
            .client
            .get_position_for_contract(self.account.id, self.contract.id)
            .await?;

        match (exchange_pos, &self.position) {
            (Some(ep), Some(lp)) => {
                if ep.net_pos != lp.contracts {
                    warn!(
                        "Position mismatch: exchange has {} contracts, we track {}",
                        ep.net_pos, lp.contracts
                    );
                }
            }
            (Some(ep), None) => {
                warn!(
                    "Unexpected position found: {} contracts @ {:.2}",
                    ep.net_pos, ep.net_price
                );
            }
            (None, Some(_)) => {
                warn!("Position closed unexpectedly, clearing local state");
                self.position = None;
            }
            (None, None) => {
                // Both flat, all good
            }
        }

        Ok(())
    }

    /// Get account info
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Get contract info
    pub fn contract(&self) -> &Contract {
        &self.contract
    }
}
