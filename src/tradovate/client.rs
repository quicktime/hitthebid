//! Tradovate API Client
//!
//! HTTP client for the Tradovate REST API with token-based authentication.

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::models::*;

/// Demo environment base URL
pub const DEMO_BASE_URL: &str = "https://demo.tradovateapi.com/v1";

/// Live environment base URL
pub const LIVE_BASE_URL: &str = "https://live.tradovateapi.com/v1";

/// Tradovate API client with automatic token management
pub struct TradovateClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
    cid: i32,
    sec: String,
    device_id: Option<String>,
    token: Option<String>,
    token_acquired_at: Option<Instant>,
}

impl TradovateClient {
    /// Create a new client from environment variables
    ///
    /// Expects:
    /// - `TRADOVATE_USERNAME` - Your Tradovate username
    /// - `TRADOVATE_PASSWORD` - Your Tradovate password
    /// - `TRADOVATE_CID` - Client ID from API settings
    /// - `TRADOVATE_SEC` - Client secret from API settings
    /// - `TRADOVATE_DEVICE_ID` (optional) - Unique device identifier
    /// - `TRADOVATE_LIVE` (optional) - Set to "true" for live trading
    pub fn from_env() -> Result<Self> {
        let username = std::env::var("TRADOVATE_USERNAME")
            .context("TRADOVATE_USERNAME environment variable not set")?;
        let password = std::env::var("TRADOVATE_PASSWORD")
            .context("TRADOVATE_PASSWORD environment variable not set")?;
        let cid = std::env::var("TRADOVATE_CID")
            .context("TRADOVATE_CID environment variable not set")?
            .parse::<i32>()
            .context("TRADOVATE_CID must be a valid integer")?;
        let sec = std::env::var("TRADOVATE_SEC")
            .context("TRADOVATE_SEC environment variable not set")?;
        let device_id = std::env::var("TRADOVATE_DEVICE_ID").ok();
        let is_live = std::env::var("TRADOVATE_LIVE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let base_url = if is_live {
            LIVE_BASE_URL.to_string()
        } else {
            DEMO_BASE_URL.to_string()
        };

        Ok(Self::new(username, password, cid, sec, device_id, base_url))
    }

    /// Create a new client for demo environment
    pub fn demo(username: String, password: String, cid: i32, sec: String) -> Self {
        Self::new(username, password, cid, sec, None, DEMO_BASE_URL.to_string())
    }

    /// Create a new client for live environment
    pub fn live(username: String, password: String, cid: i32, sec: String) -> Self {
        Self::new(username, password, cid, sec, None, LIVE_BASE_URL.to_string())
    }

    /// Create a new client with explicit configuration
    pub fn new(
        username: String,
        password: String,
        cid: i32,
        sec: String,
        device_id: Option<String>,
        base_url: String,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            base_url,
            username,
            password,
            cid,
            sec,
            device_id,
            token: None,
            token_acquired_at: None,
        }
    }

    /// Check if token needs refresh (tokens valid for 1 hour, refresh at 50 minutes)
    fn token_needs_refresh(&self) -> bool {
        match self.token_acquired_at {
            Some(acquired_at) => acquired_at.elapsed() > Duration::from_secs(50 * 60),
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

    /// Authenticate with the API and obtain an access token
    pub async fn authenticate(&mut self) -> Result<()> {
        info!("Authenticating with Tradovate API at {}...", self.base_url);

        let request = AuthRequest {
            name: self.username.clone(),
            password: self.password.clone(),
            app_id: "HitTheBid".to_string(),
            app_version: "1.0.0".to_string(),
            cid: self.cid,
            sec: self.sec.clone(),
            device_id: self.device_id.clone(),
        };

        let response = self
            .client
            .post(format!("{}/auth/accesstokenrequest", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send authentication request")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(anyhow!(
                "Authentication failed with status {}: {}",
                status,
                body
            ));
        }

        let auth_response: AuthResponse =
            serde_json::from_str(&body).context("Failed to parse authentication response")?;

        // Check for error
        if let Some(error) = auth_response.error_text {
            return Err(anyhow!("Authentication failed: {}", error));
        }

        // Check for P-Ticket (security challenge)
        if auth_response.p_ticket.is_some() {
            return Err(anyhow!(
                "Authentication requires additional verification (P-Ticket). \
                 Please complete verification through the Tradovate web interface first."
            ));
        }

        let token = auth_response
            .access_token
            .ok_or_else(|| anyhow!("No access token returned"))?;

        self.token = Some(token);
        self.token_acquired_at = Some(Instant::now());

        info!("Successfully authenticated with Tradovate");
        Ok(())
    }

    /// Renew the access token before it expires
    pub async fn renew_token(&mut self) -> Result<()> {
        let current_token = self
            .token
            .clone()
            .ok_or_else(|| anyhow!("No token to renew"))?;

        debug!("Renewing Tradovate access token...");

        let request = RenewTokenRequest {
            access_token: current_token,
        };

        let response = self
            .client
            .post(format!("{}/auth/renewaccesstoken", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send token renewal request")?;

        if !response.status().is_success() {
            // If renewal fails, try full re-authentication
            warn!("Token renewal failed, attempting full re-authentication");
            return self.authenticate().await;
        }

        let auth_response: AuthResponse = response
            .json()
            .await
            .context("Failed to parse renewal response")?;

        if let Some(new_token) = auth_response.access_token {
            self.token = Some(new_token);
            self.token_acquired_at = Some(Instant::now());
            debug!("Token renewed successfully");
        } else {
            // Fall back to full authentication
            return self.authenticate().await;
        }

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

    /// Make an authenticated GET request
    async fn get<R: serde::de::DeserializeOwned>(&self, endpoint: &str) -> Result<R> {
        let auth = self.auth_header()?;

        let response = self
            .client
            .get(format!("{}{}", self.base_url, endpoint))
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("Failed to send GET request to {}", endpoint))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("GET {} failed ({}): {}", endpoint, status, body));
        }

        response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", endpoint))
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
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to send POST request to {}", endpoint))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(anyhow!("POST {} failed ({}): {}", endpoint, status, body_text));
        }

        serde_json::from_str(&body_text)
            .with_context(|| format!("Failed to parse response from {}: {}", endpoint, body_text))
    }

    // ========================================================================
    // Account Methods
    // ========================================================================

    /// Get list of accounts
    pub async fn get_accounts(&self) -> Result<Vec<Account>> {
        debug!("Fetching accounts...");
        let accounts: Vec<Account> = self.get("/account/list").await?;
        debug!("Found {} accounts", accounts.len());
        Ok(accounts)
    }

    /// Get the first active account
    pub async fn get_first_account(&self) -> Result<Account> {
        let accounts = self.get_accounts().await?;
        accounts
            .into_iter()
            .find(|a| a.active)
            .ok_or_else(|| anyhow!("No active account found"))
    }

    // ========================================================================
    // Contract Methods
    // ========================================================================

    /// Find a contract by symbol name
    pub async fn find_contract(&self, symbol: &str) -> Result<Contract> {
        debug!("Finding contract for symbol: {}", symbol);

        // Use the contract/find endpoint
        let contracts: Vec<Contract> = self
            .get(&format!("/contract/find?name={}", symbol))
            .await?;

        contracts
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Contract '{}' not found", symbol))
    }

    /// Get contract details by ID
    pub async fn get_contract(&self, contract_id: i64) -> Result<Contract> {
        self.get(&format!("/contract/item?id={}", contract_id))
            .await
    }

    // ========================================================================
    // Order Methods
    // ========================================================================

    /// Place a market order
    pub async fn place_market_order(
        &self,
        account: &Account,
        symbol: &str,
        action: OrderAction,
        qty: i32,
    ) -> Result<Order> {
        info!(
            "Placing market order: {} {} {} @ MKT",
            action, qty, symbol
        );

        let request = PlaceOrderRequest {
            account_spec: account.name.clone(),
            account_id: account.id,
            action,
            symbol: symbol.to_string(),
            order_qty: qty,
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            time_in_force: Some(TimeInForce::Day),
            is_automated: true, // CME requirement
            custom_tag_50: None,
        };

        let order: Order = self.post("/order/placeorder", &request).await?;
        info!("Market order placed: ID {}", order.id);
        Ok(order)
    }

    /// Place a limit order
    pub async fn place_limit_order(
        &self,
        account: &Account,
        symbol: &str,
        action: OrderAction,
        qty: i32,
        price: f64,
    ) -> Result<Order> {
        info!(
            "Placing limit order: {} {} {} @ {:.2}",
            action, qty, symbol, price
        );

        let request = PlaceOrderRequest {
            account_spec: account.name.clone(),
            account_id: account.id,
            action,
            symbol: symbol.to_string(),
            order_qty: qty,
            order_type: OrderType::Limit,
            price: Some(price),
            stop_price: None,
            time_in_force: Some(TimeInForce::Day),
            is_automated: true,
            custom_tag_50: None,
        };

        let order: Order = self.post("/order/placeorder", &request).await?;
        info!("Limit order placed: ID {}", order.id);
        Ok(order)
    }

    /// Place a stop order
    pub async fn place_stop_order(
        &self,
        account: &Account,
        symbol: &str,
        action: OrderAction,
        qty: i32,
        stop_price: f64,
    ) -> Result<Order> {
        info!(
            "Placing stop order: {} {} {} @ STOP {:.2}",
            action, qty, symbol, stop_price
        );

        let request = PlaceOrderRequest {
            account_spec: account.name.clone(),
            account_id: account.id,
            action,
            symbol: symbol.to_string(),
            order_qty: qty,
            order_type: OrderType::Stop,
            price: None,
            stop_price: Some(stop_price),
            time_in_force: Some(TimeInForce::GTC),
            is_automated: true,
            custom_tag_50: None,
        };

        let order: Order = self.post("/order/placeorder", &request).await?;
        info!("Stop order placed: ID {}", order.id);
        Ok(order)
    }

    /// Cancel an order
    pub async fn cancel_order(&self, order_id: i64) -> Result<()> {
        debug!("Canceling order: {}", order_id);

        let request = CancelOrderRequest { order_id };

        let _: CommandResponse = self.post("/order/cancelorder", &request).await?;
        debug!("Order {} canceled", order_id);
        Ok(())
    }

    /// Modify an order's price
    pub async fn modify_order(
        &self,
        order_id: i64,
        new_price: Option<f64>,
        new_stop_price: Option<f64>,
    ) -> Result<()> {
        debug!(
            "Modifying order {}: price={:?}, stop={:?}",
            order_id, new_price, new_stop_price
        );

        let request = ModifyOrderRequest {
            order_id,
            order_qty: None,
            price: new_price,
            stop_price: new_stop_price,
        };

        let _: CommandResponse = self.post("/order/modifyorder", &request).await?;
        debug!("Order {} modified", order_id);
        Ok(())
    }

    /// Get all working (open) orders for an account
    pub async fn get_working_orders(&self, account_id: i64) -> Result<Vec<Order>> {
        debug!("Fetching working orders for account {}", account_id);
        self.get(&format!("/order/ldeps?masterid={}", account_id))
            .await
    }

    // ========================================================================
    // Position Methods
    // ========================================================================

    /// Get all positions for an account
    pub async fn get_positions(&self, account_id: i64) -> Result<Vec<Position>> {
        debug!("Fetching positions for account {}", account_id);
        self.get(&format!("/position/ldeps?masterid={}", account_id))
            .await
    }

    /// Get position for a specific contract
    pub async fn get_position_for_contract(
        &self,
        account_id: i64,
        contract_id: i64,
    ) -> Result<Option<Position>> {
        let positions = self.get_positions(account_id).await?;
        Ok(positions.into_iter().find(|p| p.contract_id == contract_id))
    }

    /// Flatten a position (close all contracts)
    pub async fn flatten_position(
        &self,
        account: &Account,
        symbol: &str,
        position: &Position,
    ) -> Result<Order> {
        let action = if position.net_pos > 0 {
            OrderAction::Sell
        } else {
            OrderAction::Buy
        };
        let qty = position.net_pos.abs();

        info!(
            "Flattening position: {} {} {} @ MKT",
            action, qty, symbol
        );

        self.place_market_order(account, symbol, action, qty).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_action_display() {
        assert_eq!(format!("{}", OrderAction::Buy), "Buy");
        assert_eq!(format!("{}", OrderAction::Sell), "Sell");
    }

    #[test]
    fn test_order_type_display() {
        assert_eq!(format!("{}", OrderType::Market), "Market");
        assert_eq!(format!("{}", OrderType::Stop), "Stop");
    }
}
