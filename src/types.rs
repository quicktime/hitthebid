use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub symbol: String,
    pub price: f64,
    pub size: u32,
    pub side: String, // "buy" or "sell"
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bubble {
    pub id: String,
    pub price: f64,
    pub size: u32, // Dominant side volume (aggression)
    pub side: String, // "buy" or "sell"
    pub timestamp: u64,
    pub x: f64,
    pub opacity: f64,
    #[serde(rename = "isSignificantImbalance")]
    pub is_significant_imbalance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CVDPoint {
    pub timestamp: u64,
    pub value: i64,
    pub x: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfileLevel {
    pub price: f64,
    #[serde(rename = "buyVolume")]
    pub buy_volume: u32,
    #[serde(rename = "sellVolume")]
    pub sell_volume: u32,
    #[serde(rename = "totalVolume")]
    pub total_volume: u32,
}

/// Absorption Zone - tracks absorption at a specific price level over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorptionZone {
    pub price: f64,
    #[serde(rename = "absorptionType")]
    pub absorption_type: String,
    #[serde(rename = "totalAbsorbed")]
    pub total_absorbed: i64,
    #[serde(rename = "eventCount")]
    pub event_count: u32,
    #[serde(rename = "firstSeen")]
    pub first_seen: u64,
    #[serde(rename = "lastSeen")]
    pub last_seen: u64,
    pub strength: String, // "weak", "medium", "strong", "defended"
    #[serde(rename = "atPoc")]
    pub at_poc: bool,
    #[serde(rename = "atVah")]
    pub at_vah: bool,
    #[serde(rename = "atVal")]
    pub at_val: bool,
    #[serde(rename = "againstTrend")]
    pub against_trend: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorptionEvent {
    pub timestamp: u64,
    pub price: f64,
    #[serde(rename = "absorptionType")]
    pub absorption_type: String,
    pub delta: i64,
    #[serde(rename = "priceChange")]
    pub price_change: f64,
    pub strength: String,
    #[serde(rename = "eventCount")]
    pub event_count: u32,
    #[serde(rename = "totalAbsorbed")]
    pub total_absorbed: i64,
    #[serde(rename = "atKeyLevel")]
    pub at_key_level: bool,
    #[serde(rename = "againstTrend")]
    pub against_trend: bool,
    pub x: f64,
}

/// Delta Flip Event - CVD crossing zero or reversing direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFlip {
    pub timestamp: u64,
    #[serde(rename = "flipType")]
    pub flip_type: String, // "zero_cross" or "reversal"
    pub direction: String, // "bullish" (crossing up/reversing up) or "bearish"
    #[serde(rename = "cvdBefore")]
    pub cvd_before: i64,
    #[serde(rename = "cvdAfter")]
    pub cvd_after: i64,
    pub x: f64,
}

/// Stacked Imbalances - 3+ consecutive price levels with same-direction imbalance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackedImbalance {
    pub timestamp: u64,
    pub side: String, // "buy" or "sell"
    #[serde(rename = "levelCount")]
    pub level_count: u32, // How many consecutive levels (3+)
    #[serde(rename = "priceHigh")]
    pub price_high: f64,
    #[serde(rename = "priceLow")]
    pub price_low: f64,
    #[serde(rename = "totalImbalance")]
    pub total_imbalance: i64, // Sum of imbalances across levels
    pub x: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    Bubble(Bubble),
    CVDPoint(CVDPoint),
    VolumeProfile { levels: Vec<VolumeProfileLevel> },
    Absorption(AbsorptionEvent),
    AbsorptionZones { zones: Vec<AbsorptionZone> },
    DeltaFlip(DeltaFlip),
    StackedImbalance(StackedImbalance),
    Connected { symbols: Vec<String> },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub action: String,
    pub symbol: Option<String>,
    pub min_size: Option<u32>,
}

/// Shared application state
pub struct AppState {
    pub tx: broadcast::Sender<WsMessage>,
    pub active_symbols: RwLock<HashSet<String>>,
    pub min_size: RwLock<u32>,
}
