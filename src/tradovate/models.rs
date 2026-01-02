//! Tradovate API Data Models
//!
//! Request and response types for the Tradovate REST API.

use serde::{Deserialize, Serialize};

// ============================================================================
// Authentication
// ============================================================================

/// Request body for authentication via access token request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    /// Account username
    pub name: String,
    /// Account password
    pub password: String,
    /// Application identifier
    pub app_id: String,
    /// Application version
    pub app_version: String,
    /// Client ID from API access settings
    pub cid: i32,
    /// Client secret from API access settings
    pub sec: String,
    /// Device ID (unique identifier for this device)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// Response from authentication endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    /// Access token for API calls
    #[serde(default)]
    pub access_token: Option<String>,
    /// Token expiration time in seconds
    #[serde(default)]
    pub expiration_time: Option<String>,
    /// User ID
    #[serde(default)]
    pub user_id: Option<i64>,
    /// Error text if authentication failed
    #[serde(default)]
    pub error_text: Option<String>,
    /// P-Ticket for additional security challenges
    #[serde(rename = "p-ticket", default)]
    pub p_ticket: Option<String>,
}

/// Request body for token renewal
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenewTokenRequest {
    /// Current access token to renew
    pub access_token: String,
}

// ============================================================================
// Accounts
// ============================================================================

/// Account information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Account ID (used for order placement)
    pub id: i64,
    /// Account name/spec
    pub name: String,
    /// User ID
    #[serde(default)]
    pub user_id: i64,
    /// Account type (e.g., "Customer")
    #[serde(default)]
    pub account_type: Option<String>,
    /// Whether account is active
    #[serde(default)]
    pub active: bool,
    /// Margin account ID (if applicable)
    #[serde(default)]
    pub margin_account_id: Option<i64>,
}

// ============================================================================
// Contracts
// ============================================================================

/// Contract/product information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// Contract ID
    pub id: i64,
    /// Contract name/symbol (e.g., "MNQH6")
    pub name: String,
    /// Product ID
    #[serde(default)]
    pub product_id: i64,
    /// Contract group name
    #[serde(default)]
    pub contract_group_name: Option<String>,
    /// Tick size (minimum price increment)
    #[serde(default)]
    pub price_increment: f64,
    /// Tick value in dollars
    #[serde(default)]
    pub price_increment_value: f64,
}

/// Product (instrument family) information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// Product ID
    pub id: i64,
    /// Product name (e.g., "MNQ")
    pub name: String,
    /// Full description
    #[serde(default)]
    pub description: Option<String>,
    /// Tick size
    #[serde(default)]
    pub price_increment: f64,
    /// Point value in dollars
    #[serde(default)]
    pub price_increment_value: f64,
    /// Currency
    #[serde(default)]
    pub currency: Option<String>,
}

// ============================================================================
// Orders
// ============================================================================

/// Order action (Buy or Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderAction {
    Buy,
    Sell,
}

impl std::fmt::Display for OrderAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderAction::Buy => write!(f, "Buy"),
            OrderAction::Sell => write!(f, "Sell"),
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
    TrailingStop,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::Market => write!(f, "Market"),
            OrderType::Limit => write!(f, "Limit"),
            OrderType::Stop => write!(f, "Stop"),
            OrderType::StopLimit => write!(f, "StopLimit"),
            OrderType::TrailingStop => write!(f, "TrailingStop"),
        }
    }
}

/// Time in force for orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    GTC, // Good Till Canceled
    IOC, // Immediate or Cancel
    FOK, // Fill or Kill
    GTD, // Good Till Date
}

impl std::fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeInForce::Day => write!(f, "Day"),
            TimeInForce::GTC => write!(f, "GTC"),
            TimeInForce::IOC => write!(f, "IOC"),
            TimeInForce::FOK => write!(f, "FOK"),
            TimeInForce::GTD => write!(f, "GTD"),
        }
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum OrderStatus {
    PendingNew,
    Working,
    Completed,
    Cancelled,
    Rejected,
    Expired,
    Filled,
}

/// Request to place an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderRequest {
    /// Account spec (account name)
    pub account_spec: String,
    /// Account ID
    pub account_id: i64,
    /// Buy or Sell
    pub action: OrderAction,
    /// Contract symbol (e.g., "MNQH6")
    pub symbol: String,
    /// Quantity (number of contracts)
    pub order_qty: i32,
    /// Order type
    pub order_type: OrderType,
    /// Limit price (required for Limit/StopLimit orders)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// Stop price (required for Stop/StopLimit orders)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    /// Time in force
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    /// REQUIRED for CME compliance - must be true for automated trading
    pub is_automated: bool,
    /// Custom order tag for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_tag_50: Option<String>,
}

/// Order information returned from API
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Order ID
    pub id: i64,
    /// Account ID
    pub account_id: i64,
    /// Contract ID
    #[serde(default)]
    pub contract_id: i64,
    /// Order action
    #[serde(default)]
    pub action: Option<String>,
    /// Order type
    #[serde(default)]
    pub order_type: Option<String>,
    /// Order quantity
    #[serde(default)]
    pub order_qty: i32,
    /// Limit price
    #[serde(default)]
    pub price: Option<f64>,
    /// Stop price
    #[serde(default)]
    pub stop_price: Option<f64>,
    /// Filled quantity
    #[serde(default)]
    pub filled_qty: i32,
    /// Average fill price
    #[serde(default)]
    pub avg_fill_price: Option<f64>,
    /// Order status
    #[serde(default)]
    pub status: Option<String>,
}

/// Request to cancel an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderRequest {
    /// Order ID to cancel
    pub order_id: i64,
}

/// Request to modify an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyOrderRequest {
    /// Order ID to modify
    pub order_id: i64,
    /// New quantity (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_qty: Option<i32>,
    /// New limit price (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// New stop price (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
}

// ============================================================================
// Positions
// ============================================================================

/// Position information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Position ID
    pub id: i64,
    /// Account ID
    pub account_id: i64,
    /// Contract ID
    pub contract_id: i64,
    /// Net position (positive = long, negative = short)
    #[serde(default)]
    pub net_pos: i32,
    /// Net price (average entry price)
    #[serde(default)]
    pub net_price: f64,
    /// Bought quantity today
    #[serde(default)]
    pub bought: i32,
    /// Sold quantity today
    #[serde(default)]
    pub sold: i32,
    /// Timestamp
    #[serde(default)]
    pub timestamp: Option<String>,
}

// ============================================================================
// Fill / Execution
// ============================================================================

/// Fill information (execution report)
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    /// Fill ID
    pub id: i64,
    /// Order ID
    pub order_id: i64,
    /// Contract ID
    pub contract_id: i64,
    /// Fill price
    pub price: f64,
    /// Fill quantity
    pub qty: i32,
    /// Fill timestamp
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Buy or Sell
    #[serde(default)]
    pub action: Option<String>,
}

// ============================================================================
// API Response Wrappers
// ============================================================================

/// Generic command response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    /// Command ID
    #[serde(default)]
    pub command_id: Option<i64>,
    /// Error text if command failed
    #[serde(default)]
    pub error_text: Option<String>,
    /// Order ID if placing order
    #[serde(default)]
    pub order_id: Option<i64>,
}

/// Error response from API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    /// Error code
    #[serde(default)]
    pub error_code: Option<i32>,
    /// Error text
    #[serde(default)]
    pub error_text: Option<String>,
    /// Status
    #[serde(default)]
    pub status: Option<String>,
}
