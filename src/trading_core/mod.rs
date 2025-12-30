//! Trading Core - Shared trading logic for both pipeline and web server
//!
//! This module contains the core trading strategy components:
//! - Bar aggregation from trades
//! - Impulse leg detection
//! - LVN (Low Volume Node) extraction
//! - Market state detection
//! - LVN retest signal generation
//! - Trading state machine
//! - Live trading orchestration

pub mod trades;
pub mod bars;
pub mod impulse;
pub mod lvn;
pub mod levels;
pub mod market_state;
pub mod lvn_retest;
pub mod state_machine;
pub mod trader;
pub mod cache;
pub mod daily_levels;

// Re-export commonly used types
pub use trades::{Trade, Side};
pub use bars::Bar;
pub use impulse::{ImpulseDirection, ImpulseLeg, RealTimeImpulseBuilder};
pub use lvn::LvnLevel;
pub use levels::{DailyLevels, LevelType, KeyLevel};
pub use market_state::{MarketState, MarketStateConfig, MarketStateResult};
pub use lvn_retest::{Direction, LvnSignal, LvnRetestConfig, LvnSignalGenerator};
pub use state_machine::{TradingStateMachine, StateMachineConfig, TradingState, LiveDailyLevels, StateTransition};
pub use trader::{LiveTrader, LiveConfig, TradeAction, TradingSummary};
