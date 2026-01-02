//! TopstepX API Data Models
//!
//! Request and response types for the TopstepX/ProjectX Gateway API.

use serde::{Deserialize, Serialize};

// ============================================================================
// Authentication
// ============================================================================

/// Request body for API key authentication
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    pub user_name: String,
    pub api_key: String,
}

/// Response from authentication endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub token: Option<String>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

// ============================================================================
// Accounts
// ============================================================================

/// Request to search for accounts
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAccountsRequest {
    pub only_active_accounts: bool,
}

/// Account information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub balance: f64,
    #[serde(default)]
    pub can_trade: bool,
}

/// Response from account search
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAccountsResponse {
    pub accounts: Option<Vec<Account>>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

// ============================================================================
// Contracts
// ============================================================================

/// Request to get available contracts
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableContractsRequest {
    pub live: bool,
}

/// Contract information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub tick_size: f64,
    pub tick_value: f64,
    #[serde(default)]
    pub currency: String,
}

/// Response from available contracts
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableContractsResponse {
    pub contracts: Option<Vec<Contract>>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

// ============================================================================
// Orders
// ============================================================================

/// Order type codes for TopstepX API
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OrderType {
    Limit = 1,
    Market = 2,
    Stop = 3,
    TrailingStop = 4,
}

impl Serialize for OrderType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(*self as i32)
    }
}

/// Order side codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl Serialize for Side {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(*self as i32)
    }
}

/// Bracket leg for stop loss or take profit
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BracketLeg {
    /// Number of ticks for the bracket
    pub ticks: i32,
    /// Order type for the bracket leg (usually Stop or Limit)
    #[serde(rename = "type")]
    pub order_type: i32,
}

/// Request to place an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderRequest {
    pub account_id: i64,
    pub contract_id: String,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub side: Side,
    pub size: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trail_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss_bracket: Option<BracketLeg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit_bracket: Option<BracketLeg>,
    /// Unique identifier for this order (must be unique per account)
    pub custom_tag: String,
}

/// Response from placing an order
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderResponse {
    pub order_id: Option<i64>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

/// Request to cancel an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderRequest {
    pub account_id: i64,
    pub order_id: i64,
}

/// Response from canceling an order
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderResponse {
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

/// Request to modify an order
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyOrderRequest {
    pub account_id: i64,
    pub order_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
}

/// Response from modifying an order
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyOrderResponse {
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

/// Request to search for open orders
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpenOrdersRequest {
    pub account_id: i64,
}

/// Order information from search
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: i64,
    pub account_id: i64,
    pub contract_id: String,
    #[serde(rename = "type")]
    pub order_type: i32,
    pub side: i32,
    pub size: i32,
    #[serde(default)]
    pub limit_price: Option<f64>,
    #[serde(default)]
    pub stop_price: Option<f64>,
    #[serde(default)]
    pub filled_size: i32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub custom_tag: Option<String>,
}

/// Response from searching open orders
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpenOrdersResponse {
    pub orders: Option<Vec<Order>>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

// ============================================================================
// Positions
// ============================================================================

/// Request to search for open positions
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpenPositionsRequest {
    pub account_id: i64,
}

/// Position information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub contract_id: String,
    pub net_pos: i32,
    #[serde(default)]
    pub avg_price: f64,
    #[serde(default)]
    pub unrealized_pnl: f64,
    #[serde(default)]
    pub realized_pnl: f64,
}

/// Response from searching open positions
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOpenPositionsResponse {
    pub positions: Option<Vec<Position>>,
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

/// Request to close a position
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosePositionRequest {
    pub account_id: i64,
    pub contract_id: String,
}

/// Response from closing a position
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosePositionResponse {
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}

// ============================================================================
// Generic API Response
// ============================================================================

/// Generic API response wrapper for error checking
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse {
    pub success: bool,
    pub error_code: i32,
    pub error_message: Option<String>,
}
