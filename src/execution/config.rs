//! Configuration for execution engine

use serde::{Deserialize, Serialize};

/// Execution mode determines whether orders are simulated or sent to exchange
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Simulated execution (no actual orders)
    Simulation,
    /// Paper trading via Rithmic Demo
    Paper,
    /// Live trading via Rithmic Live
    Live,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Simulation
    }
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simulation => write!(f, "Simulation"),
            Self::Paper => write!(f, "Paper"),
            Self::Live => write!(f, "Live"),
        }
    }
}

/// Configuration for the execution engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Execution mode (simulation, paper, or live)
    pub mode: ExecutionMode,

    /// Symbol to trade (e.g., "NQ.c.0" for front month NQ)
    pub symbol: String,

    /// Exchange (e.g., "CME")
    pub exchange: String,

    /// Maximum position size in contracts
    pub max_position_size: i32,

    /// Daily loss limit in points (trading stops when reached)
    pub daily_loss_limit: f64,

    /// Max losing trades per day (trading stops when reached)
    pub max_daily_losses: i32,

    /// Take profit target in points
    pub take_profit: f64,

    /// Trailing stop distance in points
    pub trailing_stop: f64,

    /// Stop buffer beyond LVN level in points
    pub stop_buffer: f64,

    /// Dollar value per point (NQ = $20, MNQ = $2)
    pub point_value: f64,

    /// Trading hours: start hour (ET)
    pub start_hour: u32,

    /// Trading hours: start minute
    pub start_minute: u32,

    /// Trading hours: end hour (ET)
    pub end_hour: u32,

    /// Trading hours: end minute
    pub end_minute: u32,

    /// Rithmic environment (Demo or Live)
    pub rithmic_env: String,

    /// Rithmic user ID
    pub rithmic_user: String,

    /// Rithmic FCM ID
    pub rithmic_fcm_id: String,

    /// Rithmic IB ID
    pub rithmic_ib_id: String,

    /// Rithmic system name
    pub rithmic_system: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Simulation,
            symbol: "NQ.c.0".to_string(),
            exchange: "CME".to_string(),
            max_position_size: 1,
            daily_loss_limit: 100.0,  // $2,000 with 1 NQ
            max_daily_losses: 3,       // Stop after 3 losing trades
            take_profit: 0.0,          // No TP - trailing stop handles exits
            trailing_stop: 1.5,        // Optimal from multi-sweep (PF 3.68, +1029 pts)
            stop_buffer: 2.0,          // Buffer for initial stop
            point_value: 20.0,         // NQ = $20/pt
            start_hour: 9,
            start_minute: 30,
            end_hour: 11,
            end_minute: 0,
            rithmic_env: "Demo".to_string(),
            rithmic_user: String::new(),
            rithmic_fcm_id: String::new(),
            rithmic_ib_id: String::new(),
            rithmic_system: "Rithmic Paper Trading".to_string(),
        }
    }
}

impl ExecutionConfig {
    /// Create config for MFF 30K Static Pro account (2 NQ max)
    pub fn mff_static_pro() -> Self {
        Self {
            max_position_size: 2,
            daily_loss_limit: 125.0,  // $2,500 DD / $20 per pt = 125 pts
            max_daily_losses: 3,      // Stop after 3 losses
            ..Default::default()
        }
    }

    /// Create config for ETF Static account (4 NQ max)
    pub fn etf_static() -> Self {
        Self {
            max_position_size: 4,
            daily_loss_limit: 100.0,  // $2,000 DD / $20 per pt = 100 pts
            max_daily_losses: 3,      // Stop after 3 losses
            ..Default::default()
        }
    }

    /// Calculate max dollar loss based on contracts
    pub fn max_dollar_loss(&self, contracts: i32) -> f64 {
        self.daily_loss_limit * self.point_value * contracts as f64
    }

    /// Check if within trading hours
    pub fn is_trading_hours(&self, hour: u32, minute: u32) -> bool {
        let current = hour * 60 + minute;
        let start = self.start_hour * 60 + self.start_minute;
        let end = self.end_hour * 60 + self.end_minute;
        current >= start && current < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_hours() {
        let config = ExecutionConfig::default();

        // Before market
        assert!(!config.is_trading_hours(9, 0));
        assert!(!config.is_trading_hours(9, 29));

        // During market
        assert!(config.is_trading_hours(9, 30));
        assert!(config.is_trading_hours(10, 0));
        assert!(config.is_trading_hours(10, 59));

        // After market
        assert!(!config.is_trading_hours(11, 0));
        assert!(!config.is_trading_hours(11, 30));
    }

    #[test]
    fn test_max_dollar_loss() {
        let config = ExecutionConfig::default();
        assert_eq!(config.max_dollar_loss(1), 2000.0);  // 100 pts * $20 * 1
        assert_eq!(config.max_dollar_loss(2), 4000.0);  // 100 pts * $20 * 2
    }
}
