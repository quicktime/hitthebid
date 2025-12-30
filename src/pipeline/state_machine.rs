//! Trading State Machine for Real-Time LVN Strategy
//!
//! Implements the CORE trading strategy:
//! 1. WAITING_FOR_BREAKOUT - Wait for price to break significant level (PDH/PDL, ONH/ONL, VAH/VAL)
//! 2. PROFILING_IMPULSE - Track the impulse leg in real-time as it forms
//! 3. HUNTING - Wait for pullback to LVN with delta confirmation
//! 4. RESET - After trade, clear ALL LVNs from that impulse and return to waiting

use crate::bars::Bar;
use crate::impulse::{ImpulseDirection, ImpulseLeg, RealTimeImpulseBuilder};
use crate::levels::DailyLevels;
use crate::lvn::LvnLevel;
use crate::trades::Trade;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Configuration for the trading state machine
#[derive(Debug, Clone)]
pub struct StateMachineConfig {
    /// Points beyond level to confirm breakout (default: 2.0)
    pub breakout_threshold: f64,
    /// Maximum bars for impulse profiling before timeout (1s bars, default: 300 = 5 min)
    pub max_impulse_bars: usize,
    /// Minimum points for a valid impulse (default: 30.0)
    pub min_impulse_size: f64,
    /// Maximum bars to hunt for retest before timeout (1s bars, default: 600 = 10 min)
    pub max_hunting_bars: usize,
    /// Minimum impulse score (out of 5) to qualify (default: 4)
    pub min_impulse_score: u8,
}

impl Default for StateMachineConfig {
    fn default() -> Self {
        Self {
            breakout_threshold: 2.0,
            max_impulse_bars: 300,   // 5 minutes
            min_impulse_size: 30.0,
            max_hunting_bars: 600,   // 10 minutes
            min_impulse_score: 4,
        }
    }
}

/// State of the trading state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingState {
    /// Waiting for price to break a significant level
    WaitingForBreakout,
    /// Breakout detected, profiling the impulse leg
    ProfilingImpulse,
    /// Impulse complete, hunting for LVN retest
    Hunting,
    /// Resetting for next cycle
    Reset,
}

impl std::fmt::Display for TradingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradingState::WaitingForBreakout => write!(f, "WAITING"),
            TradingState::ProfilingImpulse => write!(f, "PROFILING"),
            TradingState::Hunting => write!(f, "HUNTING"),
            TradingState::Reset => write!(f, "RESET"),
        }
    }
}

/// Type of breakout level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakoutLevel {
    PDH, // Prior Day High
    PDL, // Prior Day Low
    ONH, // Overnight High
    ONL, // Overnight Low
    VAH, // Value Area High
    VAL, // Value Area Low
}

impl std::fmt::Display for BreakoutLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakoutLevel::PDH => write!(f, "PDH"),
            BreakoutLevel::PDL => write!(f, "PDL"),
            BreakoutLevel::ONH => write!(f, "ONH"),
            BreakoutLevel::ONL => write!(f, "ONL"),
            BreakoutLevel::VAH => write!(f, "VAH"),
            BreakoutLevel::VAL => write!(f, "VAL"),
        }
    }
}

/// Daily reference levels for live trading
#[derive(Debug, Clone)]
pub struct LiveDailyLevels {
    pub date: NaiveDate,
    pub pdh: f64,
    pub pdl: f64,
    pub onh: f64,
    pub onl: f64,
    pub vah: f64,
    pub val: f64,
    pub session_high: f64,
    pub session_low: f64,
}

impl LiveDailyLevels {
    /// Create from DailyLevels structure
    pub fn from_daily_levels(levels: &DailyLevels) -> Self {
        Self {
            date: levels.date,
            pdh: levels.pdh,
            pdl: levels.pdl,
            onh: levels.onh,
            onl: levels.onl,
            vah: levels.vah,
            val: levels.val,
            session_high: levels.session_high,
            session_low: levels.session_low,
        }
    }

    /// Check if price has broken a significant level
    /// Returns the level type and impulse direction if a breakout occurred
    pub fn check_breakout(&self, price: f64, threshold: f64) -> Option<(BreakoutLevel, ImpulseDirection)> {
        // Check PDH/PDL first (most significant)
        if price > self.pdh + threshold {
            return Some((BreakoutLevel::PDH, ImpulseDirection::Up));
        }
        if price < self.pdl - threshold {
            return Some((BreakoutLevel::PDL, ImpulseDirection::Down));
        }

        // Check ONH/ONL
        if self.onh > 0.0 && price > self.onh + threshold {
            return Some((BreakoutLevel::ONH, ImpulseDirection::Up));
        }
        if self.onl > 0.0 && price < self.onl - threshold {
            return Some((BreakoutLevel::ONL, ImpulseDirection::Down));
        }

        // Check VAH/VAL
        if price > self.vah + threshold {
            return Some((BreakoutLevel::VAH, ImpulseDirection::Up));
        }
        if price < self.val - threshold {
            return Some((BreakoutLevel::VAL, ImpulseDirection::Down));
        }

        None
    }
}

/// Active impulse being profiled
#[derive(Debug, Clone)]
pub struct ActiveImpulse {
    /// Unique ID for this impulse
    pub id: Uuid,
    /// Direction of the impulse
    pub direction: ImpulseDirection,
    /// Level that was broken to trigger this impulse
    pub broken_level: BreakoutLevel,
    /// Builder for tracking the impulse in real-time
    pub builder: RealTimeImpulseBuilder,
    /// Trades collected during this impulse (for LVN extraction)
    pub trades: Vec<Trade>,
    /// Bar index when impulse started
    pub start_bar_idx: usize,
}

/// State transition events
#[derive(Debug, Clone)]
pub enum StateTransition {
    /// Breakout detected, starting to profile impulse
    BreakoutDetected {
        level: BreakoutLevel,
        direction: ImpulseDirection,
        price: f64,
    },
    /// Impulse profiling complete, LVNs extracted
    ImpulseComplete {
        impulse_id: Uuid,
        lvn_count: usize,
        direction: ImpulseDirection,
    },
    /// Impulse did not meet criteria or timed out
    ImpulseInvalid {
        reason: String,
    },
    /// Hunting period timed out without trade
    HuntingTimeout,
    /// Reset complete, ready for next breakout
    Reset,
}

/// The trading state machine
pub struct TradingStateMachine {
    config: StateMachineConfig,
    state: TradingState,
    daily_levels: Option<LiveDailyLevels>,
    active_impulse: Option<ActiveImpulse>,
    active_lvns: Vec<LvnLevel>,
    hunting_start_bar: Option<usize>,
    bar_count: usize,
    /// Rolling volume average for impulse scoring
    rolling_volume: Vec<u64>,
    /// Maximum rolling volume samples
    max_volume_samples: usize,
}

impl TradingStateMachine {
    /// Create a new state machine with the given config
    pub fn new(config: StateMachineConfig) -> Self {
        Self {
            config,
            state: TradingState::WaitingForBreakout,
            daily_levels: None,
            active_impulse: None,
            active_lvns: Vec::new(),
            hunting_start_bar: None,
            bar_count: 0,
            rolling_volume: Vec::with_capacity(60),
            max_volume_samples: 60, // 1 minute of 1s bars for volume average
        }
    }

    /// Set the daily levels for breakout detection
    pub fn set_daily_levels(&mut self, levels: LiveDailyLevels) {
        self.daily_levels = Some(levels);
    }

    /// Get current state
    pub fn state(&self) -> TradingState {
        self.state
    }

    /// Get active LVNs (valid during Hunting state)
    pub fn active_lvns(&self) -> &[LvnLevel] {
        &self.active_lvns
    }

    /// Get active impulse ID if profiling or hunting
    pub fn active_impulse_id(&self) -> Option<Uuid> {
        self.active_impulse.as_ref().map(|i| i.id)
    }

    /// Clear all LVNs from the current impulse (called after trade)
    pub fn clear_impulse_lvns(&mut self) {
        self.active_lvns.clear();
        self.active_impulse = None;
    }

    /// Reset to waiting for breakout state
    pub fn reset(&mut self) {
        self.state = TradingState::WaitingForBreakout;
        self.active_impulse = None;
        self.active_lvns.clear();
        self.hunting_start_bar = None;
    }

    /// Reset for new trading day
    pub fn reset_for_new_day(&mut self) {
        self.reset();
        self.bar_count = 0;
        self.rolling_volume.clear();
    }

    /// Process a trade (for LVN extraction during impulse profiling)
    pub fn process_trade(&mut self, trade: &Trade) {
        if let Some(ref mut impulse) = self.active_impulse {
            impulse.trades.push(trade.clone());
        }
    }

    /// Get the average volume from rolling samples
    fn avg_volume(&self) -> f64 {
        if self.rolling_volume.is_empty() {
            return 0.0;
        }
        self.rolling_volume.iter().sum::<u64>() as f64 / self.rolling_volume.len() as f64
    }

    /// Update rolling volume with a new bar
    fn update_rolling_volume(&mut self, bar: &Bar) {
        self.rolling_volume.push(bar.volume);
        if self.rolling_volume.len() > self.max_volume_samples {
            self.rolling_volume.remove(0);
        }
    }

    /// Process a bar and return any state transition
    pub fn process_bar(&mut self, bar: &Bar) -> Option<StateTransition> {
        self.bar_count += 1;
        self.update_rolling_volume(bar);

        match self.state {
            TradingState::WaitingForBreakout => self.process_waiting(bar),
            TradingState::ProfilingImpulse => self.process_profiling(bar),
            TradingState::Hunting => self.process_hunting(bar),
            TradingState::Reset => {
                // Reset is instantaneous
                self.state = TradingState::WaitingForBreakout;
                Some(StateTransition::Reset)
            }
        }
    }

    /// Process bar while waiting for breakout
    fn process_waiting(&mut self, bar: &Bar) -> Option<StateTransition> {
        let Some(ref levels) = self.daily_levels else {
            return None;
        };

        // Check for breakout
        if let Some((level, direction)) = levels.check_breakout(bar.close, self.config.breakout_threshold) {
            // Start profiling impulse
            let id = Uuid::new_v4();
            let builder = RealTimeImpulseBuilder::new(bar, direction);

            self.active_impulse = Some(ActiveImpulse {
                id,
                direction,
                broken_level: level,
                builder,
                trades: Vec::new(),
                start_bar_idx: self.bar_count,
            });

            self.state = TradingState::ProfilingImpulse;

            return Some(StateTransition::BreakoutDetected {
                level,
                direction,
                price: bar.close,
            });
        }

        None
    }

    /// Process bar while profiling impulse
    fn process_profiling(&mut self, bar: &Bar) -> Option<StateTransition> {
        // Compute avg_volume before borrowing active_impulse
        let avg_volume = self.avg_volume();
        let bar_count = self.bar_count;
        let max_impulse_bars = self.config.max_impulse_bars;

        let Some(ref mut impulse) = self.active_impulse else {
            // Should not happen, but recover gracefully
            self.state = TradingState::WaitingForBreakout;
            return None;
        };

        // Add bar to impulse builder
        impulse.builder.add_bar(bar);

        // Check timeout
        let elapsed_bars = bar_count - impulse.start_bar_idx;
        if elapsed_bars > max_impulse_bars {
            self.state = TradingState::Reset;
            return Some(StateTransition::ImpulseInvalid {
                reason: format!("Impulse timed out after {} bars", elapsed_bars),
            });
        }

        // Check if impulse meets minimum size
        if !impulse.builder.is_sufficient_size() {
            return None; // Keep profiling
        }

        // Check if impulse is complete (meets scoring criteria)
        // For breakout, we consider it "broke swing" since we already validated breakout
        let broke_swing = true;

        if impulse.builder.is_complete(broke_swing, avg_volume) {
            // Extract LVNs from this impulse
            let impulse_id = impulse.id;
            let direction = impulse.direction;
            let start_time = impulse.builder.start_time();
            let end_time = bar.timestamp;
            let symbol = bar.symbol.clone();

            // Use the trades collected during this impulse to extract LVNs
            let lvns = crate::lvn::extract_lvns_realtime(
                &impulse.trades,
                impulse_id,
                start_time,
                end_time,
                direction,
                &symbol,
            );

            let lvn_count = lvns.len();
            self.active_lvns = lvns;
            self.hunting_start_bar = Some(self.bar_count);
            self.state = TradingState::Hunting;

            return Some(StateTransition::ImpulseComplete {
                impulse_id,
                lvn_count,
                direction,
            });
        }

        // Check if impulse failed (reversed direction or exhausted)
        let move_size = impulse.builder.move_size();
        let max_expected = impulse.builder.end_price();
        let current_price = bar.close;

        // If price has retraced significantly, impulse is invalid
        let retrace_threshold = move_size * 0.5; // 50% retrace = invalid
        let retraced = match impulse.direction {
            ImpulseDirection::Up => current_price < max_expected - retrace_threshold,
            ImpulseDirection::Down => current_price > max_expected + retrace_threshold,
        };

        if retraced {
            self.state = TradingState::Reset;
            return Some(StateTransition::ImpulseInvalid {
                reason: "Impulse retraced >50% before completing".to_string(),
            });
        }

        None
    }

    /// Process bar while hunting for LVN retest
    fn process_hunting(&mut self, bar: &Bar) -> Option<StateTransition> {
        let Some(start_bar) = self.hunting_start_bar else {
            // Should not happen
            self.state = TradingState::WaitingForBreakout;
            return None;
        };

        // Check hunting timeout
        let elapsed_bars = self.bar_count - start_bar;
        if elapsed_bars > self.config.max_hunting_bars {
            self.state = TradingState::Reset;
            return Some(StateTransition::HuntingTimeout);
        }

        // The actual signal detection is handled by LvnSignalGenerator
        // This state machine just manages the high-level flow
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_init() {
        let sm = TradingStateMachine::new(StateMachineConfig::default());
        assert_eq!(sm.state(), TradingState::WaitingForBreakout);
    }

    #[test]
    fn test_breakout_detection() {
        let levels = LiveDailyLevels {
            date: chrono::Utc::now().date_naive(),
            pdh: 21500.0,
            pdl: 21400.0,
            onh: 0.0,
            onl: 0.0,
            vah: 21480.0,
            val: 21420.0,
            session_high: 21500.0,
            session_low: 21400.0,
        };

        // Test PDH breakout
        let result = levels.check_breakout(21503.0, 2.0);
        assert!(result.is_some());
        let (level, dir) = result.unwrap();
        assert_eq!(level, BreakoutLevel::PDH);
        assert_eq!(dir, ImpulseDirection::Up);

        // Test PDL breakout
        let result = levels.check_breakout(21397.0, 2.0);
        assert!(result.is_some());
        let (level, dir) = result.unwrap();
        assert_eq!(level, BreakoutLevel::PDL);
        assert_eq!(dir, ImpulseDirection::Down);

        // Test no breakout
        let result = levels.check_breakout(21450.0, 2.0);
        assert!(result.is_none());
    }
}
