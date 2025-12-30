//! Trading signal generator for the web frontend
//!
//! Generates entry/exit signals based on delta flips and price action.
//! This is a simplified version for the web UI - uses delta confirmation
//! to generate trading signals with stop/target levels.

use crate::types::{TradingSignal, WsMessage};
use tokio::sync::broadcast;
use tracing::{info, debug};

/// Direction of a trade
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Long,
    Short,
}

/// Current position state
#[derive(Debug, Clone)]
pub struct Position {
    pub direction: Direction,
    pub entry_price: f64,
    pub stop: f64,
    pub target: f64,
    pub entry_time: u64,
}

/// Configuration for the trading observer
#[derive(Debug, Clone)]
pub struct TradingConfig {
    pub take_profit_points: f64,
    pub stop_loss_points: f64,
    pub min_delta_threshold: i64,
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            take_profit_points: 20.0,
            stop_loss_points: 10.0,
            min_delta_threshold: 50, // Minimum delta to trigger signal
        }
    }
}

/// Observes market data and generates trading signals
pub struct TradingObserver {
    config: TradingConfig,
    current_position: Option<Position>,
    last_signal_time: u64,
    cumulative_delta: i64,
    last_price: f64,
    signal_count: u32,
}

impl TradingObserver {
    pub fn new(config: TradingConfig) -> Self {
        Self {
            config,
            current_position: None,
            last_signal_time: 0,
            cumulative_delta: 0,
            last_price: 0.0,
            signal_count: 0,
        }
    }

    /// Process a delta flip event and potentially generate a trading signal
    pub fn on_delta_flip(
        &mut self,
        direction: &str,
        price: f64,
        timestamp: u64,
        tx: &broadcast::Sender<WsMessage>,
    ) {
        // Don't signal too frequently (at least 30 seconds between signals)
        if timestamp - self.last_signal_time < 30_000 {
            return;
        }

        // If we have an open position, don't take new entries
        if self.current_position.is_some() {
            return;
        }

        let dir = if direction == "bullish" {
            Direction::Long
        } else {
            Direction::Short
        };

        self.generate_entry_signal(dir, price, timestamp, tx);
    }

    /// Update with current price to check for exits
    pub fn on_price_update(
        &mut self,
        price: f64,
        timestamp: u64,
        tx: &broadcast::Sender<WsMessage>,
    ) {
        self.last_price = price;

        if let Some(ref pos) = self.current_position.clone() {
            let pnl = match pos.direction {
                Direction::Long => price - pos.entry_price,
                Direction::Short => pos.entry_price - price,
            };

            // Check for target hit
            if pnl >= self.config.take_profit_points {
                self.generate_exit_signal(price, timestamp, pnl, "Target hit", tx);
            }
            // Check for stop hit
            else if pnl <= -self.config.stop_loss_points {
                self.generate_exit_signal(price, timestamp, pnl, "Stop loss", tx);
            }
        }
    }

    fn generate_entry_signal(
        &mut self,
        direction: Direction,
        price: f64,
        timestamp: u64,
        tx: &broadcast::Sender<WsMessage>,
    ) {
        let (stop, target) = match direction {
            Direction::Long => (
                price - self.config.stop_loss_points,
                price + self.config.take_profit_points,
            ),
            Direction::Short => (
                price + self.config.stop_loss_points,
                price - self.config.take_profit_points,
            ),
        };

        let position = Position {
            direction,
            entry_price: price,
            stop,
            target,
            entry_time: timestamp,
        };

        self.current_position = Some(position);
        self.last_signal_time = timestamp;
        self.signal_count += 1;

        let dir_str = match direction {
            Direction::Long => "long",
            Direction::Short => "short",
        };

        info!(
            "ENTRY SIGNAL #{}: {} @ {:.2} | Stop: {:.2} | Target: {:.2}",
            self.signal_count, dir_str.to_uppercase(), price, stop, target
        );

        let signal = TradingSignal {
            timestamp,
            signal_type: "entry".to_string(),
            direction: dir_str.to_string(),
            price,
            stop: Some(stop),
            target: Some(target),
            pnl_points: None,
            reason: None,
            x: 0.92,
        };

        let _ = tx.send(WsMessage::TradingSignal(signal));
    }

    fn generate_exit_signal(
        &mut self,
        price: f64,
        timestamp: u64,
        pnl: f64,
        reason: &str,
        tx: &broadcast::Sender<WsMessage>,
    ) {
        if let Some(ref pos) = self.current_position {
            let dir_str = match pos.direction {
                Direction::Long => "long",
                Direction::Short => "short",
            };

            let emoji = if pnl >= 0.0 { "WIN" } else { "LOSS" };
            info!(
                "EXIT {}: {} @ {:.2} | P&L: {:.2} pts | {}",
                emoji, dir_str.to_uppercase(), price, pnl, reason
            );

            let signal = TradingSignal {
                timestamp,
                signal_type: "exit".to_string(),
                direction: dir_str.to_string(),
                price,
                stop: None,
                target: None,
                pnl_points: Some(pnl),
                reason: Some(reason.to_string()),
                x: 0.92,
            };

            let _ = tx.send(WsMessage::TradingSignal(signal));
        }

        self.current_position = None;
    }

    /// Check if we have an open position
    pub fn has_position(&self) -> bool {
        self.current_position.is_some()
    }

    /// Get current position info
    pub fn current_position(&self) -> Option<&Position> {
        self.current_position.as_ref()
    }
}
