//! Position management and P&L tracking

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Local};
use std::collections::VecDeque;
use super::order::{OrderSide, BracketOrder};

/// Individual trade record for P&L history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    /// Entry price
    pub entry_price: f64,
    /// Exit price
    pub exit_price: f64,
    /// Trade side
    pub side: OrderSide,
    /// Number of contracts
    pub quantity: i32,
    /// P&L in points
    pub pnl_points: f64,
    /// Entry time
    pub entry_time: DateTime<Utc>,
    /// Exit time
    pub exit_time: DateTime<Utc>,
    /// LVN level that triggered the trade
    pub lvn_level: f64,
}

/// Daily P&L summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPnL {
    /// Date (YYYY-MM-DD)
    pub date: String,
    /// Gross P&L in points
    pub gross_pnl: f64,
    /// Number of trades
    pub trade_count: i32,
    /// Number of winning trades
    pub wins: i32,
    /// Number of losing trades
    pub losses: i32,
    /// Largest win in points
    pub largest_win: f64,
    /// Largest loss in points
    pub largest_loss: f64,
    /// Max drawdown in points (from session high)
    pub max_drawdown: f64,
    /// Peak balance reached
    pub peak_balance: f64,
}

impl Default for DailyPnL {
    fn default() -> Self {
        Self {
            date: Local::now().format("%Y-%m-%d").to_string(),
            gross_pnl: 0.0,
            trade_count: 0,
            wins: 0,
            losses: 0,
            largest_win: 0.0,
            largest_loss: 0.0,
            max_drawdown: 0.0,
            peak_balance: 0.0,
        }
    }
}

/// Position manager tracks open positions and P&L
#[derive(Debug)]
pub struct PositionManager {
    /// Symbol being traded
    symbol: String,

    /// Current net position (positive = long, negative = short)
    net_position: i32,

    /// Average entry price of current position
    avg_entry_price: Option<f64>,

    /// Active bracket orders
    active_brackets: Vec<BracketOrder>,

    /// Completed trade history
    trade_history: VecDeque<TradeRecord>,

    /// Today's P&L tracking
    daily_pnl: DailyPnL,

    /// Running balance (for prop firm tracking)
    running_balance: f64,

    /// Starting balance
    starting_balance: f64,

    /// Point value (NQ = $20)
    point_value: f64,

    /// Max history to keep
    max_history: usize,
}

impl PositionManager {
    /// Create a new position manager
    pub fn new(symbol: &str, starting_balance: f64, point_value: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            net_position: 0,
            avg_entry_price: None,
            active_brackets: Vec::new(),
            trade_history: VecDeque::new(),
            daily_pnl: DailyPnL::default(),
            running_balance: starting_balance,
            starting_balance,
            point_value,
            max_history: 1000,
        }
    }

    /// Get current net position
    pub fn net_position(&self) -> i32 {
        self.net_position
    }

    /// Check if flat (no position)
    pub fn is_flat(&self) -> bool {
        self.net_position == 0
    }

    /// Get average entry price
    pub fn avg_entry_price(&self) -> Option<f64> {
        self.avg_entry_price
    }

    /// Get running balance
    pub fn running_balance(&self) -> f64 {
        self.running_balance
    }

    /// Get today's P&L in points
    pub fn daily_pnl_points(&self) -> f64 {
        self.daily_pnl.gross_pnl
    }

    /// Get today's P&L in dollars
    pub fn daily_pnl_dollars(&self) -> f64 {
        self.daily_pnl.gross_pnl * self.point_value
    }

    /// Get drawdown from starting balance
    pub fn drawdown(&self) -> f64 {
        if self.running_balance < self.starting_balance {
            self.starting_balance - self.running_balance
        } else {
            0.0
        }
    }

    /// Get drawdown in points
    pub fn drawdown_points(&self) -> f64 {
        self.drawdown() / self.point_value
    }

    /// Register a new bracket order
    pub fn add_bracket(&mut self, bracket: BracketOrder) {
        self.active_brackets.push(bracket);
    }

    /// Get active bracket by ID
    pub fn get_bracket(&self, id: &uuid::Uuid) -> Option<&BracketOrder> {
        self.active_brackets.iter().find(|b| &b.id == id)
    }

    /// Get mutable active bracket by ID
    pub fn get_bracket_mut(&mut self, id: &uuid::Uuid) -> Option<&mut BracketOrder> {
        self.active_brackets.iter_mut().find(|b| &b.id == id)
    }

    /// Get all active brackets
    pub fn active_brackets(&self) -> &[BracketOrder] {
        &self.active_brackets
    }

    /// Record entry fill
    pub fn record_entry_fill(&mut self, bracket_id: &uuid::Uuid, fill_price: f64, quantity: i32, side: OrderSide) {
        // Update net position
        let signed_qty = if side == OrderSide::Buy { quantity } else { -quantity };

        if self.net_position == 0 {
            self.avg_entry_price = Some(fill_price);
        } else if self.net_position.signum() == signed_qty.signum() {
            // Adding to position - weighted average
            let old_value = self.avg_entry_price.unwrap_or(fill_price) * self.net_position.abs() as f64;
            let new_value = fill_price * quantity as f64;
            self.avg_entry_price = Some((old_value + new_value) / (self.net_position.abs() + quantity) as f64);
        }

        self.net_position += signed_qty;

        // Update bracket state
        if let Some(bracket) = self.get_bracket_mut(bracket_id) {
            bracket.entry_price = Some(fill_price);
        }
    }

    /// Record exit fill and calculate P&L
    pub fn record_exit_fill(&mut self, bracket_id: &uuid::Uuid, fill_price: f64) -> Option<TradeRecord> {
        let bracket = self.active_brackets.iter_mut().find(|b| &b.id == bracket_id)?;

        let entry_price = bracket.entry_price?;
        let side = bracket.position_side();
        let quantity = bracket.entry.quantity;

        // Calculate P&L
        let pnl_points = if side == OrderSide::Buy {
            (fill_price - entry_price) * quantity as f64
        } else {
            (entry_price - fill_price) * quantity as f64
        };

        // Update running balance
        let pnl_dollars = pnl_points * self.point_value;
        self.running_balance += pnl_dollars;

        // Update daily P&L
        self.daily_pnl.gross_pnl += pnl_points;
        self.daily_pnl.trade_count += 1;

        if pnl_points > 0.0 {
            self.daily_pnl.wins += 1;
            if pnl_points > self.daily_pnl.largest_win {
                self.daily_pnl.largest_win = pnl_points;
            }
        } else {
            self.daily_pnl.losses += 1;
            if pnl_points < self.daily_pnl.largest_loss {
                self.daily_pnl.largest_loss = pnl_points;
            }
        }

        // Update peak and drawdown
        if self.running_balance > self.daily_pnl.peak_balance {
            self.daily_pnl.peak_balance = self.running_balance;
        }
        let dd = self.daily_pnl.peak_balance - self.running_balance;
        if dd > self.daily_pnl.max_drawdown {
            self.daily_pnl.max_drawdown = dd;
        }

        // Update net position
        let signed_qty = if side == OrderSide::Buy { quantity } else { -quantity };
        self.net_position -= signed_qty;

        if self.net_position == 0 {
            self.avg_entry_price = None;
        }

        // Create trade record
        let record = TradeRecord {
            entry_price,
            exit_price: fill_price,
            side,
            quantity,
            pnl_points,
            entry_time: bracket.created_at,
            exit_time: Utc::now(),
            lvn_level: bracket.lvn_level,
        };

        // Add to history
        self.trade_history.push_back(record.clone());
        if self.trade_history.len() > self.max_history {
            self.trade_history.pop_front();
        }

        // Mark bracket complete
        bracket.complete(fill_price);

        Some(record)
    }

    /// Remove completed brackets
    pub fn cleanup_completed(&mut self) {
        self.active_brackets.retain(|b| !b.is_terminal());
    }

    /// Get unrealized P&L for all open positions
    pub fn unrealized_pnl(&self, current_price: f64) -> f64 {
        self.active_brackets
            .iter()
            .filter_map(|b| b.unrealized_pnl(current_price))
            .sum()
    }

    /// Get total P&L (realized + unrealized)
    pub fn total_pnl(&self, current_price: f64) -> f64 {
        self.daily_pnl.gross_pnl + self.unrealized_pnl(current_price)
    }

    /// Reset daily P&L (call at start of new trading day)
    pub fn reset_daily(&mut self) {
        self.daily_pnl = DailyPnL::default();
        self.daily_pnl.peak_balance = self.running_balance;
    }

    /// Get trade history
    pub fn trade_history(&self) -> &VecDeque<TradeRecord> {
        &self.trade_history
    }

    /// Get daily P&L summary
    pub fn daily_summary(&self) -> &DailyPnL {
        &self.daily_pnl
    }

    /// Get win rate
    pub fn win_rate(&self) -> f64 {
        if self.daily_pnl.trade_count == 0 {
            0.0
        } else {
            self.daily_pnl.wins as f64 / self.daily_pnl.trade_count as f64
        }
    }

    /// Get statistics summary
    pub fn stats_summary(&self) -> String {
        format!(
            "Balance: ${:.2} | Day P&L: {:.1} pts (${:.2}) | Trades: {} | WR: {:.1}%",
            self.running_balance,
            self.daily_pnl.gross_pnl,
            self.daily_pnl_dollars(),
            self.daily_pnl.trade_count,
            self.win_rate() * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_tracking() {
        let mut pm = PositionManager::new("NQ.c.0", 50000.0, 20.0);

        assert!(pm.is_flat());
        assert_eq!(pm.net_position(), 0);
    }

    #[test]
    fn test_pnl_calculation() {
        let mut pm = PositionManager::new("NQ.c.0", 50000.0, 20.0);

        // Create and track a bracket
        let mut bracket = BracketOrder::new_long("NQ.c.0", "CME", 1, 21500.0, 1.5);
        let bracket_id = bracket.id;
        pm.add_bracket(bracket);

        // Record entry
        pm.record_entry_fill(&bracket_id, 21505.0, 1, OrderSide::Buy);
        assert_eq!(pm.net_position(), 1);

        // Record exit with 10 pt profit
        let record = pm.record_exit_fill(&bracket_id, 21515.0).unwrap();
        assert_eq!(record.pnl_points, 10.0);
        assert_eq!(pm.running_balance, 50200.0); // 10 pts * $20

        assert!(pm.is_flat());
        assert_eq!(pm.daily_pnl.wins, 1);
    }
}
