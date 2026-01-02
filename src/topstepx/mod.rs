//! TopstepX API Integration
//!
//! This module provides integration with the TopstepX/ProjectX Gateway API
//! for executing trades on Topstep prop firm accounts.
//!
//! # Components
//!
//! - [`client`] - HTTP client with JWT authentication
//! - [`models`] - Request/response data types
//! - [`executor`] - TradeAction to API call translation
//!
//! # Usage
//!
//! ```rust,ignore
//! use topstepx::{TopstepClient, TopstepExecutor, TradeAction, Direction};
//!
//! // Create client from environment variables
//! let client = TopstepClient::from_env()?;
//!
//! // Create executor for NQ futures
//! let mut executor = TopstepExecutor::new(client, "NQ").await?;
//!
//! // Execute a trade action
//! executor.execute(TradeAction::Enter {
//!     direction: Direction::Long,
//!     price: 21500.0,
//!     stop: 21495.0,
//!     target: 21510.0,
//!     contracts: 1,
//! }).await?;
//! ```

pub mod client;
pub mod executor;
pub mod models;

// Re-export commonly used types
pub use client::TopstepClient;
pub use executor::{Direction, TopstepExecutor, TradeAction};
pub use models::{Account, Contract, Order, Position, Side};
