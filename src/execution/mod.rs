//! Execution module for automated trading via Rithmic API
//!
//! This module provides the infrastructure for executing trades
//! based on signals from the LVN Retest strategy.

mod config;
mod connection;
mod order;
mod position;
mod engine;

pub use config::{ExecutionConfig, ExecutionMode};
pub use connection::RithmicConnection;
pub use order::{Order, OrderState, OrderSide, BracketOrder};
pub use position::PositionManager;
pub use engine::{ExecutionEngine, TradingSignal};
