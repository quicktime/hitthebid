//! Trade types for trading core

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Raw trade from market data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub ts_event: DateTime<Utc>,
    pub price: f64,
    pub size: u64,
    pub side: Side,
    pub symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}
