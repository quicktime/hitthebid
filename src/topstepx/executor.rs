//! TopstepX Trade Executor
//!
//! Translates TradeAction signals to TopstepX API calls.
//! Follows the same pattern as IbOrderManager in ib_execution.rs.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::client::TopstepClient;
use super::models::Side;

/// Direction of a trade (matches pipeline::lvn_retest::Direction)
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

/// TopstepX trade executor
///
/// Manages order submission and position tracking for the TopstepX API.
/// Similar to IbOrderManager but for TopstepX.
pub struct TopstepExecutor {
    client: TopstepClient,
    account_id: i64,
    contract_id: String,
    tick_size: f64,
    tick_value: f64,
    position: Option<ExecutorPosition>,
    order_tags: HashMap<String, i64>, // custom_tag -> order_id
}

impl TopstepExecutor {
    /// Create a new executor
    ///
    /// Authenticates with TopstepX and looks up the account and contract.
    pub async fn new(mut client: TopstepClient, symbol: &str) -> Result<Self> {
        // Ensure authenticated
        client.ensure_authenticated().await?;

        // Get account ID
        let account_id = client
            .get_first_account_id()
            .await
            .context("Failed to get account ID")?;
        info!("Using TopstepX account ID: {}", account_id);

        // Find contract
        let contracts = client.get_contracts(true).await?;
        let contract = contracts
            .iter()
            .find(|c| c.name.contains(symbol) || c.id.contains(symbol))
            .ok_or_else(|| anyhow!("Contract for '{}' not found", symbol))?;

        info!(
            "Using contract: {} ({}) - tick size: {}, tick value: {}",
            contract.name, contract.id, contract.tick_size, contract.tick_value
        );

        Ok(Self {
            client,
            account_id,
            contract_id: contract.id.clone(),
            tick_size: contract.tick_size,
            tick_value: contract.tick_value,
            position: None,
            order_tags: HashMap::new(),
        })
    }

    /// Generate a unique order tag
    fn generate_tag(&self) -> String {
        format!("htb-{}", Uuid::new_v4().to_string()[..8].to_string())
    }

    /// Convert price difference to ticks
    fn price_to_ticks(&self, price_diff: f64) -> i32 {
        (price_diff.abs() / self.tick_size).round() as i32
    }

    /// Execute a trade action
    pub async fn execute(&mut self, action: TradeAction) -> Result<()> {
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
                // Nothing to do - signal will be executed on next bar
                Ok(())
            }
        }
    }

    /// Submit a bracket order (market entry with stop and target)
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

        let side = match direction {
            Direction::Long => Side::Buy,
            Direction::Short => Side::Sell,
        };

        // Calculate bracket distances in ticks
        // For a long: stop is below entry, target is above
        // For a short: stop is above entry, target is below
        // We'll estimate entry price as midpoint for tick calculation
        let estimated_entry = (stop_price + target_price) / 2.0;
        let stop_ticks = self.price_to_ticks(estimated_entry - stop_price);
        let target_ticks = self.price_to_ticks(target_price - estimated_entry);

        info!(
            "Submitting bracket order: {} {} contracts @ MKT | Stop: {:.2} ({} ticks) | Target: {:.2} ({} ticks)",
            if matches!(direction, Direction::Long) { "BUY" } else { "SELL" },
            contracts,
            stop_price,
            stop_ticks,
            target_price,
            target_ticks
        );

        let tag = self.generate_tag();

        // Place market order with bracket
        let order_id = self
            .client
            .place_market_order(
                self.account_id,
                &self.contract_id,
                side,
                contracts,
                Some(stop_ticks),
                Some(target_ticks),
                &tag,
            )
            .await?;

        // Track the order
        self.order_tags.insert(tag.clone(), order_id);

        // Update position state
        self.position = Some(ExecutorPosition {
            direction,
            contracts,
            entry_price: estimated_entry, // Will be updated when we get fill
            stop_order_id: None,          // Bracket orders are managed by exchange
            target_order_id: None,
        });

        info!("Bracket order submitted: ID {}", order_id);
        Ok(())
    }

    /// Modify the stop loss price
    async fn modify_stop(&mut self, new_stop: f64) -> Result<()> {
        let pos = match &self.position {
            Some(p) => p,
            None => {
                debug!("No position to modify stop for");
                return Ok(());
            }
        };

        // Find the stop order from our open orders
        let orders = self.client.get_open_orders(self.account_id).await?;

        // Look for stop order (type 3 = Stop)
        let stop_order = orders.iter().find(|o| o.order_type == 3);

        match stop_order {
            Some(order) => {
                debug!("Modifying stop order {} to {:.2}", order.id, new_stop);
                self.client
                    .modify_order(self.account_id, order.id, Some(new_stop), None)
                    .await?;
                info!("Stop modified to {:.2}", new_stop);
            }
            None => {
                // If no stop order found, we might need to place one
                // This can happen if the original bracket stop was filled/canceled
                warn!("No stop order found to modify, placing new stop order");

                let side = match pos.direction {
                    Direction::Long => Side::Sell,
                    Direction::Short => Side::Buy,
                };

                let tag = self.generate_tag();
                let order_id = self
                    .client
                    .place_stop_order(
                        self.account_id,
                        &self.contract_id,
                        side,
                        pos.contracts,
                        new_stop,
                        &tag,
                    )
                    .await?;

                self.order_tags.insert(tag, order_id);
                info!("New stop order placed: ID {} at {:.2}", order_id, new_stop);
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

        // Close the position via API
        self.client
            .close_position(self.account_id, &self.contract_id)
            .await?;

        self.position = None;
        info!("Position closed");
        Ok(())
    }

    /// Flatten all positions and cancel all orders
    async fn flatten_all(&mut self) -> Result<()> {
        // Cancel all open orders
        self.cancel_all_orders().await?;

        // Close position if any
        if self.position.is_some() {
            if let Err(e) = self
                .client
                .close_position(self.account_id, &self.contract_id)
                .await
            {
                error!("Failed to close position: {}", e);
            }
        }

        self.position = None;
        self.order_tags.clear();

        info!("All positions flattened");
        Ok(())
    }

    /// Cancel all open orders
    async fn cancel_all_orders(&mut self) -> Result<()> {
        let orders = self.client.get_open_orders(self.account_id).await?;

        for order in orders {
            if let Err(e) = self.client.cancel_order(self.account_id, order.id).await {
                warn!("Failed to cancel order {}: {}", order.id, e);
            } else {
                debug!("Canceled order {}", order.id);
            }
        }

        self.order_tags.clear();
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
        let positions = self.client.get_open_positions(self.account_id).await?;

        // Find position for our contract
        let exchange_pos = positions
            .iter()
            .find(|p| p.contract_id == self.contract_id);

        match (exchange_pos, &self.position) {
            (Some(ep), Some(lp)) => {
                // Verify positions match
                if ep.net_pos != lp.contracts {
                    warn!(
                        "Position mismatch: exchange has {} contracts, we track {}",
                        ep.net_pos, lp.contracts
                    );
                }
            }
            (Some(ep), None) => {
                // Exchange has position but we don't track it
                warn!(
                    "Unexpected position found: {} contracts @ {:.2}",
                    ep.net_pos, ep.avg_price
                );
            }
            (None, Some(_)) => {
                // We think we have a position but exchange doesn't
                warn!("Position closed unexpectedly, clearing local state");
                self.position = None;
            }
            (None, None) => {
                // Both flat, all good
            }
        }

        Ok(())
    }
}
