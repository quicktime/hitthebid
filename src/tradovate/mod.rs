//! Tradovate API Integration
//!
//! This module provides integration with the Tradovate REST API
//! for executing trades on Tradovate brokerage accounts.
//!
//! # Components
//!
//! - [`client`] - HTTP client with token-based authentication
//! - [`models`] - Request/response data types
//! - [`executor`] - TradeAction to API call translation
//!
//! # Environment Variables
//!
//! - `TRADOVATE_USERNAME` - Your Tradovate username
//! - `TRADOVATE_PASSWORD` - Your Tradovate password
//! - `TRADOVATE_CID` - Client ID from API settings
//! - `TRADOVATE_SEC` - Client secret from API settings
//! - `TRADOVATE_DEVICE_ID` (optional) - Unique device identifier
//! - `TRADOVATE_LIVE` (optional) - Set to "true" for live trading
//!
//! # Usage
//!
//! ```rust,ignore
//! use tradovate::{TradovateClient, TradovateExecutor, TradeAction, Direction};
//!
//! // Create client from environment variables
//! let client = TradovateClient::from_env()?;
//!
//! // Create executor for MNQ futures
//! let mut executor = TradovateExecutor::new(client, "MNQH6").await?;
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
//!
//! # API Endpoints Used
//!
//! - `POST /auth/accesstokenrequest` - Authentication
//! - `GET /account/list` - Get accounts
//! - `GET /contract/find?name=...` - Find contracts
//! - `POST /order/placeorder` - Place orders
//! - `POST /order/cancelorder` - Cancel orders
//! - `POST /order/modifyorder` - Modify orders
//! - `GET /position/ldeps?masterid=...` - Get positions

pub mod client;
pub mod executor;
pub mod models;

// Re-export commonly used types
pub use client::TradovateClient;
pub use executor::{Direction, TradovateExecutor, TradeAction};
pub use models::{Account, Contract, Order, OrderAction, Position};
