//! Execution engine - main interface for automated trading

use anyhow::{Result, bail};
use tokio::sync::broadcast;
use tracing::{info, warn, debug};
use uuid::Uuid;

use super::config::{ExecutionConfig, ExecutionMode};
use super::connection::{RithmicConnection, RithmicEvent, ConnectionState};
use super::order::{OrderSide, BracketOrder, BracketState};
use super::position::PositionManager;

/// Events emitted by the execution engine
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    /// Signal received, order submitted
    SignalExecuted {
        bracket_id: Uuid,
        side: OrderSide,
        lvn_level: f64,
    },
    /// Entry order filled
    EntryFilled {
        bracket_id: Uuid,
        fill_price: f64,
        quantity: i32,
    },
    /// Exit order filled (stop or target)
    ExitFilled {
        bracket_id: Uuid,
        fill_price: f64,
        pnl_points: f64,
        exit_type: String,
    },
    /// Trailing stop updated
    TrailingStopUpdated {
        bracket_id: Uuid,
        new_stop: f64,
    },
    /// Daily loss limit reached
    DailyLimitReached {
        pnl_points: f64,
    },
    /// Max daily losses reached
    MaxLossesReached {
        loss_count: i32,
    },
    /// Position flattened
    PositionFlattened {
        reason: String,
    },
    /// Error occurred
    Error {
        message: String,
    },
}

/// Trading signal from strategy
#[derive(Debug, Clone)]
pub struct TradingSignal {
    /// Signal side (long or short)
    pub side: OrderSide,
    /// LVN level that triggered the signal
    pub lvn_level: f64,
    /// Current price when signal fired
    pub current_price: f64,
    /// Delta imbalance at signal
    pub delta: f64,
}

/// Execution engine orchestrates order management
pub struct ExecutionEngine {
    config: ExecutionConfig,
    connection: RithmicConnection,
    position_manager: PositionManager,
    event_tx: broadcast::Sender<ExecutionEvent>,
    daily_loss_limit_hit: bool,
    max_losses_hit: bool,
}

impl ExecutionEngine {
    /// Create a new execution engine
    pub fn new(config: ExecutionConfig) -> Self {
        let connection = RithmicConnection::new(config.clone());
        let position_manager = PositionManager::new(
            &config.symbol,
            50000.0, // Default starting balance
            config.point_value,
        );

        let (event_tx, _) = broadcast::channel(1000);

        Self {
            config,
            connection,
            position_manager,
            event_tx,
            daily_loss_limit_hit: false,
            max_losses_hit: false,
        }
    }

    /// Create with custom starting balance (for prop firm tracking)
    pub fn with_balance(config: ExecutionConfig, starting_balance: f64) -> Self {
        let connection = RithmicConnection::new(config.clone());
        let position_manager = PositionManager::new(
            &config.symbol,
            starting_balance,
            config.point_value,
        );

        let (event_tx, _) = broadcast::channel(1000);

        Self {
            config,
            connection,
            position_manager,
            event_tx,
            daily_loss_limit_hit: false,
            max_losses_hit: false,
        }
    }

    /// Connect to Rithmic
    pub async fn connect(&mut self) -> Result<()> {
        self.connection.connect().await
    }

    /// Disconnect from Rithmic
    pub async fn disconnect(&mut self) -> Result<()> {
        // Cancel all orders first
        self.flatten_all("Disconnecting").await?;
        self.connection.disconnect().await
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }

    /// Get connection state
    pub fn connection_state(&self) -> ConnectionState {
        self.connection.state()
    }

    /// Subscribe to execution events
    pub fn subscribe(&self) -> broadcast::Receiver<ExecutionEvent> {
        self.event_tx.subscribe()
    }

    /// Execute a trading signal
    pub async fn execute_signal(&mut self, signal: &TradingSignal, quantity: i32) -> Result<Uuid> {
        // Check daily loss limit
        if self.daily_loss_limit_hit {
            bail!("Daily loss limit already reached, no new trades");
        }

        // Check max daily losses
        if self.max_losses_hit {
            bail!("Max daily losses ({}) reached, no new trades", self.config.max_daily_losses);
        }

        // Check if we're already at max position
        let current_position = self.position_manager.net_position().abs();
        if current_position + quantity > self.config.max_position_size {
            bail!(
                "Would exceed max position size ({} + {} > {})",
                current_position,
                quantity,
                self.config.max_position_size
            );
        }

        // Create bracket order
        let bracket = if signal.side == OrderSide::Buy {
            BracketOrder::new_long(
                &self.config.symbol,
                &self.config.exchange,
                quantity,
                signal.lvn_level,
                self.config.stop_buffer,
            )
        } else {
            BracketOrder::new_short(
                &self.config.symbol,
                &self.config.exchange,
                quantity,
                signal.lvn_level,
                self.config.stop_buffer,
            )
        };

        let bracket_id = bracket.id;

        // Submit entry order
        let side_str = signal.side.to_string();
        let order_id = self.connection.submit_market_order(
            &self.config.symbol,
            &side_str,
            quantity,
        ).await?;

        info!(
            "Signal executed: {} {} @ market (LVN: {:.2})",
            side_str, quantity, signal.lvn_level
        );

        // Track the bracket
        self.position_manager.add_bracket(bracket);

        // Emit event
        let _ = self.event_tx.send(ExecutionEvent::SignalExecuted {
            bracket_id,
            side: signal.side,
            lvn_level: signal.lvn_level,
        });

        // In simulation mode, simulate immediate fill
        if self.config.mode == ExecutionMode::Simulation {
            self.simulate_entry_fill(bracket_id, signal.current_price).await?;
        }

        Ok(bracket_id)
    }

    /// Simulate entry fill (for simulation/paper mode)
    async fn simulate_entry_fill(&mut self, bracket_id: Uuid, fill_price: f64) -> Result<()> {
        let bracket = self.position_manager.get_bracket_mut(&bracket_id)
            .ok_or_else(|| anyhow::anyhow!("Bracket not found"))?;

        let side = bracket.position_side();
        let quantity = bracket.entry.quantity;

        // Record the fill
        self.position_manager.record_entry_fill(&bracket_id, fill_price, quantity, side);

        // Set exit orders on the bracket
        if let Some(bracket) = self.position_manager.get_bracket_mut(&bracket_id) {
            bracket.set_exit_orders(
                fill_price,
                self.config.take_profit,
                self.config.stop_buffer,
            );

            // Submit stop and target orders
            let stop_price = bracket.stop_loss.as_ref()
                .and_then(|o| o.stop_price)
                .unwrap_or(0.0);
            let target_price = bracket.take_profit.as_ref()
                .and_then(|o| o.limit_price)
                .unwrap_or(0.0);

            let exit_side = side.opposite().to_string();

            // Submit stop order
            self.connection.submit_stop_order(
                &self.config.symbol,
                &exit_side,
                quantity,
                stop_price,
            ).await?;

            // Submit target order
            self.connection.submit_limit_order(
                &self.config.symbol,
                &exit_side,
                quantity,
                target_price,
            ).await?;

            info!(
                "Bracket orders placed: Stop @ {:.2}, Target @ {:.2}",
                stop_price, target_price
            );
        }

        // Emit event
        let _ = self.event_tx.send(ExecutionEvent::EntryFilled {
            bracket_id,
            fill_price,
            quantity,
        });

        Ok(())
    }

    /// Update trailing stops based on current price
    pub async fn update_trailing_stops(&mut self, current_price: f64) -> Result<()> {
        let brackets: Vec<Uuid> = self.position_manager.active_brackets()
            .iter()
            .filter(|b| b.state == BracketState::PositionOpen)
            .map(|b| b.id)
            .collect();

        for bracket_id in brackets {
            if let Some(bracket) = self.position_manager.get_bracket_mut(&bracket_id) {
                if let Some(new_stop) = bracket.update_trailing_stop(current_price, self.config.trailing_stop) {
                    // Update stop order with exchange
                    if let Some(stop_order) = &bracket.stop_loss {
                        if let Some(exchange_id) = &stop_order.exchange_order_id {
                            self.connection.modify_order(exchange_id, Some(new_stop), None).await?;
                        }
                    }

                    debug!("Trailing stop updated to {:.2} for bracket {}", new_stop, bracket_id);

                    let _ = self.event_tx.send(ExecutionEvent::TrailingStopUpdated {
                        bracket_id,
                        new_stop,
                    });
                }
            }
        }

        Ok(())
    }

    /// Check for stop/target hits in simulation mode
    pub fn check_exit_triggers(&mut self, current_price: f64) -> Vec<(Uuid, f64, String)> {
        let mut exits = Vec::new();

        let brackets: Vec<(Uuid, OrderSide, Option<f64>, Option<f64>)> = self.position_manager
            .active_brackets()
            .iter()
            .filter(|b| b.state == BracketState::PositionOpen)
            .map(|b| {
                let stop = b.stop_loss.as_ref().and_then(|o| o.stop_price);
                let target = b.take_profit.as_ref().and_then(|o| o.limit_price);
                (b.id, b.position_side(), stop, target)
            })
            .collect();

        for (bracket_id, side, stop, target) in brackets {
            // Check stop hit
            if let Some(stop_price) = stop {
                let hit = match side {
                    OrderSide::Buy => current_price <= stop_price,
                    OrderSide::Sell => current_price >= stop_price,
                };
                if hit {
                    exits.push((bracket_id, stop_price, "STOP".to_string()));
                    continue;
                }
            }

            // Check target hit
            if let Some(target_price) = target {
                let hit = match side {
                    OrderSide::Buy => current_price >= target_price,
                    OrderSide::Sell => current_price <= target_price,
                };
                if hit {
                    exits.push((bracket_id, target_price, "TARGET".to_string()));
                }
            }
        }

        exits
    }

    /// Process an exit fill
    pub fn process_exit_fill(&mut self, bracket_id: Uuid, fill_price: f64, exit_type: &str) {
        if let Some(record) = self.position_manager.record_exit_fill(&bracket_id, fill_price) {
            info!(
                "{} hit @ {:.2}: P&L {:.1} pts (${:.2})",
                exit_type,
                fill_price,
                record.pnl_points,
                record.pnl_points * self.config.point_value
            );

            let _ = self.event_tx.send(ExecutionEvent::ExitFilled {
                bracket_id,
                fill_price,
                pnl_points: record.pnl_points,
                exit_type: exit_type.to_string(),
            });

            // Check daily loss limit
            self.check_daily_limit();
        }
    }

    /// Check if daily loss limit or max losses hit
    fn check_daily_limit(&mut self) {
        // Check P&L limit
        let daily_pnl = self.position_manager.daily_pnl_points();
        if daily_pnl <= -self.config.daily_loss_limit {
            self.daily_loss_limit_hit = true;
            warn!(
                "Daily loss limit reached: {:.1} pts (limit: -{:.1})",
                daily_pnl, self.config.daily_loss_limit
            );

            let _ = self.event_tx.send(ExecutionEvent::DailyLimitReached {
                pnl_points: daily_pnl,
            });
        }

        // Check max losses
        let loss_count = self.position_manager.daily_summary().losses;
        if loss_count >= self.config.max_daily_losses {
            self.max_losses_hit = true;
            warn!(
                "Max daily losses reached: {} losses (limit: {})",
                loss_count, self.config.max_daily_losses
            );

            let _ = self.event_tx.send(ExecutionEvent::MaxLossesReached {
                loss_count,
            });
        }
    }

    /// Check if daily loss limit has been hit
    pub fn is_daily_limit_hit(&self) -> bool {
        self.daily_loss_limit_hit
    }

    /// Check if max daily losses has been hit
    pub fn is_max_losses_hit(&self) -> bool {
        self.max_losses_hit
    }

    /// Check if trading is stopped (either limit hit)
    pub fn is_trading_stopped(&self) -> bool {
        self.daily_loss_limit_hit || self.max_losses_hit
    }

    /// Flatten all positions
    pub async fn flatten_all(&mut self, reason: &str) -> Result<()> {
        info!("Flattening all positions: {}", reason);

        // Cancel all working orders
        self.connection.cancel_all_orders().await?;

        // Close any open positions
        let position = self.position_manager.net_position();
        if position != 0 {
            let side = if position > 0 { "SELL" } else { "BUY" };
            let quantity = position.abs();

            self.connection.submit_market_order(
                &self.config.symbol,
                side,
                quantity,
            ).await?;
        }

        let _ = self.event_tx.send(ExecutionEvent::PositionFlattened {
            reason: reason.to_string(),
        });

        Ok(())
    }

    /// Get position manager reference
    pub fn position_manager(&self) -> &PositionManager {
        &self.position_manager
    }

    /// Get position manager mutable reference
    pub fn position_manager_mut(&mut self) -> &mut PositionManager {
        &mut self.position_manager
    }

    /// Get current config
    pub fn config(&self) -> &ExecutionConfig {
        &self.config
    }

    /// Get daily P&L in points
    pub fn daily_pnl(&self) -> f64 {
        self.position_manager.daily_pnl_points()
    }

    /// Get running balance
    pub fn balance(&self) -> f64 {
        self.position_manager.running_balance()
    }

    /// Reset for new trading day
    pub fn reset_daily(&mut self) {
        self.position_manager.reset_daily();
        self.daily_loss_limit_hit = false;
        self.max_losses_hit = false;
        info!("Reset for new trading day");
    }

    /// Print status summary
    pub fn print_status(&self) {
        info!("{}", self.position_manager.stats_summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_simulation() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            ..Default::default()
        };

        let mut engine = ExecutionEngine::new(config);
        engine.connect().await.unwrap();

        assert!(engine.is_connected());
    }

    #[tokio::test]
    async fn test_execute_signal() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            daily_loss_limit: 100.0,
            take_profit: 30.0,
            trailing_stop: 6.0,
            stop_buffer: 1.5,
            ..Default::default()
        };

        let mut engine = ExecutionEngine::new(config);
        engine.connect().await.unwrap();

        let signal = TradingSignal {
            side: OrderSide::Buy,
            lvn_level: 21500.0,
            current_price: 21505.0,
            delta: 100.0,
        };

        let bracket_id = engine.execute_signal(&signal, 1).await.unwrap();

        // Check position was opened
        assert_eq!(engine.position_manager().net_position(), 1);

        // Check bracket was created with correct levels
        let bracket = engine.position_manager().get_bracket(&bracket_id).unwrap();
        assert_eq!(bracket.entry_price, Some(21505.0));
        assert_eq!(bracket.stop_loss.as_ref().unwrap().stop_price, Some(21498.5)); // 21500 - 1.5
    }

    #[tokio::test]
    async fn test_daily_loss_limit() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            daily_loss_limit: 10.0, // 10 pts limit
            ..Default::default()
        };

        let mut engine = ExecutionEngine::with_balance(config, 50000.0);
        engine.connect().await.unwrap();

        // Execute a signal
        let signal = TradingSignal {
            side: OrderSide::Buy,
            lvn_level: 21500.0,
            current_price: 21505.0,
            delta: 100.0,
        };

        let bracket_id = engine.execute_signal(&signal, 1).await.unwrap();

        // Simulate a 15 pt loss (exceeds limit)
        engine.process_exit_fill(bracket_id, 21490.0, "STOP");

        assert!(engine.is_daily_limit_hit());

        // Should not be able to execute new signals
        let result = engine.execute_signal(&signal, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_max_daily_losses() {
        let config = ExecutionConfig {
            mode: ExecutionMode::Simulation,
            max_daily_losses: 3,
            daily_loss_limit: 1000.0, // High limit so we hit loss count first
            ..Default::default()
        };

        let mut engine = ExecutionEngine::with_balance(config, 50000.0);
        engine.connect().await.unwrap();

        let signal = TradingSignal {
            side: OrderSide::Buy,
            lvn_level: 21500.0,
            current_price: 21505.0,
            delta: 100.0,
        };

        // Take 3 small losses
        for i in 0..3 {
            let bracket_id = engine.execute_signal(&signal, 1).await.unwrap();
            // Small 2 pt loss each time
            engine.process_exit_fill(bracket_id, 21503.0, "STOP");

            if i < 2 {
                assert!(!engine.is_max_losses_hit(), "Should not be hit after {} losses", i + 1);
            }
        }

        // After 3 losses, should be stopped
        assert!(engine.is_max_losses_hit());
        assert!(engine.is_trading_stopped());

        // Should not be able to execute new signals
        let result = engine.execute_signal(&signal, 1).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Max daily losses"));
    }
}
