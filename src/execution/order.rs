//! Order types and state machine for bracket orders

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Order side (buy or sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn opposite(&self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Buy => write!(f, "BUY"),
            Self::Sell => write!(f, "SELL"),
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
}

/// Order state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    /// Order created but not yet submitted
    Pending,
    /// Order submitted to exchange
    Submitted,
    /// Order acknowledged by exchange
    Working,
    /// Order partially filled
    PartiallyFilled,
    /// Order completely filled
    Filled,
    /// Order cancelled
    Cancelled,
    /// Order rejected by exchange
    Rejected,
}

impl std::fmt::Display for OrderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "PENDING"),
            Self::Submitted => write!(f, "SUBMITTED"),
            Self::Working => write!(f, "WORKING"),
            Self::PartiallyFilled => write!(f, "PARTIAL"),
            Self::Filled => write!(f, "FILLED"),
            Self::Cancelled => write!(f, "CANCELLED"),
            Self::Rejected => write!(f, "REJECTED"),
        }
    }
}

/// Individual order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Unique order ID (client-side)
    pub id: Uuid,

    /// Exchange order ID (set after submission)
    pub exchange_order_id: Option<String>,

    /// Symbol
    pub symbol: String,

    /// Exchange
    pub exchange: String,

    /// Order side
    pub side: OrderSide,

    /// Order type
    pub order_type: OrderType,

    /// Quantity in contracts
    pub quantity: i32,

    /// Filled quantity
    pub filled_quantity: i32,

    /// Limit price (for limit orders)
    pub limit_price: Option<f64>,

    /// Stop price (for stop orders)
    pub stop_price: Option<f64>,

    /// Current state
    pub state: OrderState,

    /// Average fill price
    pub avg_fill_price: Option<f64>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Order {
    /// Create a new market order
    pub fn market(symbol: &str, exchange: &str, side: OrderSide, quantity: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            exchange_order_id: None,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            side,
            order_type: OrderType::Market,
            quantity,
            filled_quantity: 0,
            limit_price: None,
            stop_price: None,
            state: OrderState::Pending,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new stop order
    pub fn stop(symbol: &str, exchange: &str, side: OrderSide, quantity: i32, stop_price: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            exchange_order_id: None,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            side,
            order_type: OrderType::Stop,
            quantity,
            filled_quantity: 0,
            limit_price: None,
            stop_price: Some(stop_price),
            state: OrderState::Pending,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new limit order
    pub fn limit(symbol: &str, exchange: &str, side: OrderSide, quantity: i32, limit_price: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            exchange_order_id: None,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            side,
            order_type: OrderType::Limit,
            quantity,
            filled_quantity: 0,
            limit_price: Some(limit_price),
            stop_price: None,
            state: OrderState::Pending,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, OrderState::Filled | OrderState::Cancelled | OrderState::Rejected)
    }

    /// Check if order is active (can be cancelled/modified)
    pub fn is_active(&self) -> bool {
        matches!(self.state, OrderState::Working | OrderState::PartiallyFilled)
    }

    /// Update order state
    pub fn update_state(&mut self, state: OrderState) {
        self.state = state;
        self.updated_at = Utc::now();
    }

    /// Record a fill
    pub fn record_fill(&mut self, fill_quantity: i32, fill_price: f64) {
        let prev_value = self.avg_fill_price.unwrap_or(0.0) * self.filled_quantity as f64;
        let new_value = fill_price * fill_quantity as f64;
        self.filled_quantity += fill_quantity;
        self.avg_fill_price = Some((prev_value + new_value) / self.filled_quantity as f64);
        self.updated_at = Utc::now();

        if self.filled_quantity >= self.quantity {
            self.state = OrderState::Filled;
        } else {
            self.state = OrderState::PartiallyFilled;
        }
    }
}

/// Bracket order state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketState {
    /// Entry order pending
    PendingEntry,
    /// Entry order working
    EntryWorking,
    /// Position open, stop/target working
    PositionOpen,
    /// Exiting position
    Exiting,
    /// Bracket complete (position closed)
    Complete,
    /// Bracket cancelled (entry not filled)
    Cancelled,
}

/// Bracket order (entry + stop + target as OCO)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketOrder {
    /// Unique bracket ID
    pub id: Uuid,

    /// Entry order
    pub entry: Order,

    /// Stop loss order (created after entry fill)
    pub stop_loss: Option<Order>,

    /// Take profit order (created after entry fill)
    pub take_profit: Option<Order>,

    /// Current bracket state
    pub state: BracketState,

    /// Entry price (set after fill)
    pub entry_price: Option<f64>,

    /// Exit price (set after exit fill)
    pub exit_price: Option<f64>,

    /// LVN level that triggered this trade
    pub lvn_level: f64,

    /// Realized P&L in points
    pub realized_pnl: Option<f64>,

    /// High water mark for trailing stop
    pub high_water_mark: Option<f64>,

    /// Low water mark for trailing stop
    pub low_water_mark: Option<f64>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Close timestamp
    pub closed_at: Option<DateTime<Utc>>,
}

impl BracketOrder {
    /// Create a new bracket order for a long trade
    pub fn new_long(
        symbol: &str,
        exchange: &str,
        quantity: i32,
        lvn_level: f64,
        _stop_buffer: f64,
    ) -> Self {
        let entry = Order::market(symbol, exchange, OrderSide::Buy, quantity);

        Self {
            id: Uuid::new_v4(),
            entry,
            stop_loss: None,
            take_profit: None,
            state: BracketState::PendingEntry,
            entry_price: None,
            exit_price: None,
            lvn_level,
            realized_pnl: None,
            high_water_mark: None,
            low_water_mark: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    /// Create a new bracket order for a short trade
    pub fn new_short(
        symbol: &str,
        exchange: &str,
        quantity: i32,
        lvn_level: f64,
        _stop_buffer: f64,
    ) -> Self {
        let entry = Order::market(symbol, exchange, OrderSide::Sell, quantity);

        Self {
            id: Uuid::new_v4(),
            entry,
            stop_loss: None,
            take_profit: None,
            state: BracketState::PendingEntry,
            entry_price: None,
            exit_price: None,
            lvn_level,
            realized_pnl: None,
            high_water_mark: None,
            low_water_mark: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    /// Set stop and target after entry fill
    pub fn set_exit_orders(
        &mut self,
        entry_price: f64,
        take_profit_pts: f64,
        stop_buffer: f64,
    ) {
        self.entry_price = Some(entry_price);

        let is_long = self.entry.side == OrderSide::Buy;

        // Calculate stop loss price (beyond LVN level)
        let stop_price = if is_long {
            self.lvn_level - stop_buffer
        } else {
            self.lvn_level + stop_buffer
        };

        // Calculate take profit price
        let tp_price = if is_long {
            entry_price + take_profit_pts
        } else {
            entry_price - take_profit_pts
        };

        // Create stop loss order
        self.stop_loss = Some(Order::stop(
            &self.entry.symbol,
            &self.entry.exchange,
            self.entry.side.opposite(),
            self.entry.quantity,
            stop_price,
        ));

        // Create take profit order
        self.take_profit = Some(Order::limit(
            &self.entry.symbol,
            &self.entry.exchange,
            self.entry.side.opposite(),
            self.entry.quantity,
            tp_price,
        ));

        // Initialize water marks for trailing stop
        if is_long {
            self.high_water_mark = Some(entry_price);
        } else {
            self.low_water_mark = Some(entry_price);
        }

        self.state = BracketState::PositionOpen;
    }

    /// Update trailing stop based on current price
    pub fn update_trailing_stop(&mut self, current_price: f64, trailing_pts: f64) -> Option<f64> {
        let is_long = self.entry.side == OrderSide::Buy;

        if is_long {
            // Update high water mark
            if let Some(hwm) = self.high_water_mark {
                if current_price > hwm {
                    self.high_water_mark = Some(current_price);
                }
            }

            // Calculate new stop based on HWM
            if let (Some(hwm), Some(stop)) = (self.high_water_mark, &mut self.stop_loss) {
                let new_stop = hwm - trailing_pts;
                if let Some(current_stop) = stop.stop_price {
                    if new_stop > current_stop {
                        stop.stop_price = Some(new_stop);
                        stop.updated_at = Utc::now();
                        return Some(new_stop);
                    }
                }
            }
        } else {
            // Update low water mark
            if let Some(lwm) = self.low_water_mark {
                if current_price < lwm {
                    self.low_water_mark = Some(current_price);
                }
            }

            // Calculate new stop based on LWM
            if let (Some(lwm), Some(stop)) = (self.low_water_mark, &mut self.stop_loss) {
                let new_stop = lwm + trailing_pts;
                if let Some(current_stop) = stop.stop_price {
                    if new_stop < current_stop {
                        stop.stop_price = Some(new_stop);
                        stop.updated_at = Utc::now();
                        return Some(new_stop);
                    }
                }
            }
        }

        None
    }

    /// Mark bracket as complete with exit price
    pub fn complete(&mut self, exit_price: f64) {
        self.exit_price = Some(exit_price);
        self.closed_at = Some(Utc::now());
        self.state = BracketState::Complete;

        // Calculate realized P&L
        if let Some(entry) = self.entry_price {
            let pnl = if self.entry.side == OrderSide::Buy {
                exit_price - entry
            } else {
                entry - exit_price
            };
            self.realized_pnl = Some(pnl * self.entry.quantity as f64);
        }
    }

    /// Check if bracket is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, BracketState::Complete | BracketState::Cancelled)
    }

    /// Get position side
    pub fn position_side(&self) -> OrderSide {
        self.entry.side
    }

    /// Get current unrealized P&L in points
    pub fn unrealized_pnl(&self, current_price: f64) -> Option<f64> {
        self.entry_price.map(|entry| {
            let pnl = if self.entry.side == OrderSide::Buy {
                current_price - entry
            } else {
                entry - current_price
            };
            pnl * self.entry.quantity as f64
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bracket_long() {
        let mut bracket = BracketOrder::new_long("NQ.c.0", "CME", 1, 21500.0, 1.5);

        // Simulate entry fill
        bracket.set_exit_orders(21505.0, 30.0, 1.5);

        assert_eq!(bracket.entry_price, Some(21505.0));
        assert_eq!(bracket.stop_loss.as_ref().unwrap().stop_price, Some(21498.5)); // 21500 - 1.5
        assert_eq!(bracket.take_profit.as_ref().unwrap().limit_price, Some(21535.0)); // 21505 + 30
    }

    #[test]
    fn test_trailing_stop_long() {
        let mut bracket = BracketOrder::new_long("NQ.c.0", "CME", 1, 21500.0, 1.5);
        bracket.set_exit_orders(21505.0, 30.0, 1.5);

        // Price moves up, stop should trail
        let new_stop = bracket.update_trailing_stop(21515.0, 6.0);
        assert_eq!(new_stop, Some(21509.0)); // 21515 - 6

        // Price moves up more
        let new_stop = bracket.update_trailing_stop(21520.0, 6.0);
        assert_eq!(new_stop, Some(21514.0)); // 21520 - 6

        // Price moves down, stop should NOT move
        let new_stop = bracket.update_trailing_stop(21518.0, 6.0);
        assert_eq!(new_stop, None);
    }

    #[test]
    fn test_unrealized_pnl() {
        let mut bracket = BracketOrder::new_long("NQ.c.0", "CME", 2, 21500.0, 1.5);
        bracket.set_exit_orders(21505.0, 30.0, 1.5);

        // Price up 10 pts with 2 contracts = 20 pts P&L
        assert_eq!(bracket.unrealized_pnl(21515.0), Some(20.0));

        // Price down 5 pts with 2 contracts = -10 pts P&L
        assert_eq!(bracket.unrealized_pnl(21500.0), Some(-10.0));
    }
}
