//! TopstepX API Client
//!
//! HTTP client for the TopstepX/ProjectX Gateway API with JWT authentication.

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::models::*;

/// Default API base URL for TopstepX demo environment
pub const DEFAULT_BASE_URL: &str = "https://gateway-api-demo.s2f.projectx.com";

/// TopstepX API client with automatic token management
pub struct TopstepClient {
    client: Client,
    base_url: String,
    username: String,
    api_key: String,
    token: Option<String>,
    token_acquired_at: Option<Instant>,
}

impl TopstepClient {
    /// Create a new client from environment variables
    ///
    /// Expects:
    /// - `TOPSTEP_USERNAME` - Your TopstepX username
    /// - `TOPSTEP_API_KEY` - Your TopstepX API key
    /// - `TOPSTEP_BASE_URL` (optional) - API base URL, defaults to demo environment
    pub fn from_env() -> Result<Self> {
        let username = std::env::var("TOPSTEP_USERNAME")
            .context("TOPSTEP_USERNAME environment variable not set")?;
        let api_key = std::env::var("TOPSTEP_API_KEY")
            .context("TOPSTEP_API_KEY environment variable not set")?;
        let base_url = std::env::var("TOPSTEP_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        Ok(Self::new(username, api_key, base_url))
    }

    /// Create a new client with explicit credentials
    pub fn new(username: String, api_key: String, base_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            base_url,
            username,
            api_key,
            token: None,
            token_acquired_at: None,
        }
    }

    /// Check if token needs refresh (tokens typically valid for ~24 hours, refresh at 23 hours)
    fn token_needs_refresh(&self) -> bool {
        match self.token_acquired_at {
            Some(acquired_at) => acquired_at.elapsed() > Duration::from_secs(23 * 60 * 60),
            None => true,
        }
    }

    /// Ensure we have a valid token, refreshing if necessary
    pub async fn ensure_authenticated(&mut self) -> Result<()> {
        if self.token.is_none() || self.token_needs_refresh() {
            self.authenticate().await?;
        }
        Ok(())
    }

    /// Authenticate with the API and obtain a JWT token
    pub async fn authenticate(&mut self) -> Result<()> {
        info!("Authenticating with TopstepX API...");

        let request = AuthRequest {
            user_name: self.username.clone(),
            api_key: self.api_key.clone(),
        };

        let response = self
            .client
            .post(format!("{}/api/Auth/loginKey", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send authentication request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Authentication failed with status {}: {}",
                status,
                body
            ));
        }

        let auth_response: AuthResponse = response
            .json()
            .await
            .context("Failed to parse authentication response")?;

        if !auth_response.success {
            return Err(anyhow!(
                "Authentication failed: {} (code: {})",
                auth_response.error_message.unwrap_or_default(),
                auth_response.error_code
            ));
        }

        self.token = auth_response.token;
        self.token_acquired_at = Some(Instant::now());

        info!("Successfully authenticated with TopstepX");
        Ok(())
    }

    /// Get the authorization header value
    fn auth_header(&self) -> Result<String> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!("Not authenticated - call authenticate() first"))?;
        Ok(format!("Bearer {}", token))
    }

    /// Make an authenticated POST request
    async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R> {
        let auth = self.auth_header()?;

        let response = self
            .client
            .post(format!("{}{}", self.base_url, endpoint))
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", endpoint))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Request to {} failed ({}): {}", endpoint, status, body));
        }

        response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", endpoint))
    }

    // ========================================================================
    // Account Methods
    // ========================================================================

    /// Search for active accounts
    pub async fn search_accounts(&self) -> Result<Vec<Account>> {
        debug!("Searching for active accounts...");

        let request = SearchAccountsRequest {
            only_active_accounts: true,
        };

        let response: SearchAccountsResponse = self.post("/api/Account/search", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to search accounts: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let accounts = response.accounts.unwrap_or_default();
        debug!("Found {} active accounts", accounts.len());
        Ok(accounts)
    }

    /// Get the first active account ID (convenience method)
    pub async fn get_first_account_id(&self) -> Result<i64> {
        let accounts = self.search_accounts().await?;
        accounts
            .first()
            .map(|a| a.id)
            .ok_or_else(|| anyhow!("No active accounts found"))
    }

    // ========================================================================
    // Contract Methods
    // ========================================================================

    /// Get available contracts
    pub async fn get_contracts(&self, live: bool) -> Result<Vec<Contract>> {
        debug!("Fetching available contracts (live={})...", live);

        let request = AvailableContractsRequest { live };

        let response: AvailableContractsResponse =
            self.post("/api/Contract/available", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to get contracts: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let contracts = response.contracts.unwrap_or_default();
        debug!("Found {} contracts", contracts.len());
        Ok(contracts)
    }

    /// Find contract ID by symbol prefix (e.g., "NQ" for E-mini Nasdaq)
    pub async fn find_contract_id(&self, symbol_prefix: &str) -> Result<String> {
        let contracts = self.get_contracts(true).await?;

        // Look for contract matching the symbol prefix
        for contract in &contracts {
            if contract.name.starts_with(symbol_prefix) || contract.id.contains(symbol_prefix) {
                debug!("Found contract: {} ({})", contract.name, contract.id);
                return Ok(contract.id.clone());
            }
        }

        // Log available contracts for debugging
        warn!(
            "Contract '{}' not found. Available contracts: {:?}",
            symbol_prefix,
            contracts.iter().map(|c| &c.name).collect::<Vec<_>>()
        );

        Err(anyhow!("Contract '{}' not found", symbol_prefix))
    }

    // ========================================================================
    // Order Methods
    // ========================================================================

    /// Place a market order with optional bracket (stop loss + take profit)
    pub async fn place_market_order(
        &self,
        account_id: i64,
        contract_id: &str,
        side: Side,
        size: i32,
        stop_loss_ticks: Option<i32>,
        take_profit_ticks: Option<i32>,
        custom_tag: &str,
    ) -> Result<i64> {
        debug!(
            "Placing market order: {} {} @ MKT (SL: {:?} ticks, TP: {:?} ticks)",
            size,
            if matches!(side, Side::Buy) { "BUY" } else { "SELL" },
            stop_loss_ticks,
            take_profit_ticks
        );

        let request = PlaceOrderRequest {
            account_id,
            contract_id: contract_id.to_string(),
            order_type: OrderType::Market,
            side,
            size,
            limit_price: None,
            stop_price: None,
            trail_price: None,
            stop_loss_bracket: stop_loss_ticks.map(|ticks| BracketLeg {
                ticks,
                order_type: OrderType::Stop as i32,
            }),
            take_profit_bracket: take_profit_ticks.map(|ticks| BracketLeg {
                ticks,
                order_type: OrderType::Limit as i32,
            }),
            custom_tag: custom_tag.to_string(),
        };

        let response: PlaceOrderResponse = self.post("/api/Order/place", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to place order: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let order_id = response
            .order_id
            .ok_or_else(|| anyhow!("Order placed but no order ID returned"))?;

        info!("Order placed successfully: ID {}", order_id);
        Ok(order_id)
    }

    /// Place a stop order
    pub async fn place_stop_order(
        &self,
        account_id: i64,
        contract_id: &str,
        side: Side,
        size: i32,
        stop_price: f64,
        custom_tag: &str,
    ) -> Result<i64> {
        debug!(
            "Placing stop order: {} {} @ {}",
            size,
            if matches!(side, Side::Buy) { "BUY" } else { "SELL" },
            stop_price
        );

        let request = PlaceOrderRequest {
            account_id,
            contract_id: contract_id.to_string(),
            order_type: OrderType::Stop,
            side,
            size,
            limit_price: None,
            stop_price: Some(stop_price),
            trail_price: None,
            stop_loss_bracket: None,
            take_profit_bracket: None,
            custom_tag: custom_tag.to_string(),
        };

        let response: PlaceOrderResponse = self.post("/api/Order/place", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to place stop order: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let order_id = response
            .order_id
            .ok_or_else(|| anyhow!("Order placed but no order ID returned"))?;

        debug!("Stop order placed: ID {}", order_id);
        Ok(order_id)
    }

    /// Place a limit order
    pub async fn place_limit_order(
        &self,
        account_id: i64,
        contract_id: &str,
        side: Side,
        size: i32,
        limit_price: f64,
        custom_tag: &str,
    ) -> Result<i64> {
        debug!(
            "Placing limit order: {} {} @ {}",
            size,
            if matches!(side, Side::Buy) { "BUY" } else { "SELL" },
            limit_price
        );

        let request = PlaceOrderRequest {
            account_id,
            contract_id: contract_id.to_string(),
            order_type: OrderType::Limit,
            side,
            size,
            limit_price: Some(limit_price),
            stop_price: None,
            trail_price: None,
            stop_loss_bracket: None,
            take_profit_bracket: None,
            custom_tag: custom_tag.to_string(),
        };

        let response: PlaceOrderResponse = self.post("/api/Order/place", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to place limit order: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let order_id = response
            .order_id
            .ok_or_else(|| anyhow!("Order placed but no order ID returned"))?;

        debug!("Limit order placed: ID {}", order_id);
        Ok(order_id)
    }

    /// Cancel an order
    pub async fn cancel_order(&self, account_id: i64, order_id: i64) -> Result<()> {
        debug!("Canceling order: {}", order_id);

        let request = CancelOrderRequest {
            account_id,
            order_id,
        };

        let response: CancelOrderResponse = self.post("/api/Order/cancel", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to cancel order: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        debug!("Order {} canceled", order_id);
        Ok(())
    }

    /// Modify an order's price
    pub async fn modify_order(
        &self,
        account_id: i64,
        order_id: i64,
        new_stop_price: Option<f64>,
        new_limit_price: Option<f64>,
    ) -> Result<()> {
        debug!(
            "Modifying order {}: stop={:?}, limit={:?}",
            order_id, new_stop_price, new_limit_price
        );

        let request = ModifyOrderRequest {
            account_id,
            order_id,
            size: None,
            limit_price: new_limit_price,
            stop_price: new_stop_price,
        };

        let response: ModifyOrderResponse = self.post("/api/Order/modify", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to modify order: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        debug!("Order {} modified", order_id);
        Ok(())
    }

    /// Get open orders for an account
    pub async fn get_open_orders(&self, account_id: i64) -> Result<Vec<Order>> {
        debug!("Fetching open orders for account {}", account_id);

        let request = SearchOpenOrdersRequest { account_id };

        let response: SearchOpenOrdersResponse =
            self.post("/api/Order/searchOpen", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to get open orders: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let orders = response.orders.unwrap_or_default();
        debug!("Found {} open orders", orders.len());
        Ok(orders)
    }

    // ========================================================================
    // Position Methods
    // ========================================================================

    /// Get open positions for an account
    pub async fn get_open_positions(&self, account_id: i64) -> Result<Vec<Position>> {
        debug!("Fetching open positions for account {}", account_id);

        let request = SearchOpenPositionsRequest { account_id };

        let response: SearchOpenPositionsResponse =
            self.post("/api/Position/searchOpen", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to get open positions: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        let positions = response.positions.unwrap_or_default();
        debug!("Found {} open positions", positions.len());
        Ok(positions)
    }

    /// Close a position for a specific contract
    pub async fn close_position(&self, account_id: i64, contract_id: &str) -> Result<()> {
        info!("Closing position for contract: {}", contract_id);

        let request = ClosePositionRequest {
            account_id,
            contract_id: contract_id.to_string(),
        };

        let response: ClosePositionResponse =
            self.post("/api/Position/closeContract", &request).await?;

        if !response.success {
            return Err(anyhow!(
                "Failed to close position: {} (code: {})",
                response.error_message.unwrap_or_default(),
                response.error_code
            ));
        }

        info!("Position closed for contract: {}", contract_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_type_serialization() {
        let market = OrderType::Market;
        let json = serde_json::to_string(&market).unwrap();
        assert_eq!(json, "2");

        let stop = OrderType::Stop;
        let json = serde_json::to_string(&stop).unwrap();
        assert_eq!(json, "3");
    }

    #[test]
    fn test_side_serialization() {
        let buy = Side::Buy;
        let json = serde_json::to_string(&buy).unwrap();
        assert_eq!(json, "0");

        let sell = Side::Sell;
        let json = serde_json::to_string(&sell).unwrap();
        assert_eq!(json, "1");
    }
}
