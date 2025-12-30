//! LVN Retest Strategy - Fabio Valentini Trend Model
//!
//! Based on the Trend Model from Fabio Valentini's methodology.
//!
//! The edge: When market is trending, LVNs from impulse legs act as
//! pullback entry points. We enter WITH the trend, not against it.
//!
//! Setup (Trend Model):
//! 1. Impulse leg breaks structure (creates LVNs)
//! 2. Price pulls back to LVN
//! 3. At LVN: look for AGGRESSION IN TREND DIRECTION (continuation)
//! 4. Enter WITH the impulse direction
//! 5. Stop: 1-2 points beyond LVN (structure-based)
//! 6. Target: POC of prior balance or next key level
//!
//! Key insight: We're joining trapped traders covering, not fading.
//! - Impulse UP → LVN is SUPPORT → bullish aggression → LONG
//! - Impulse DOWN → LVN is RESISTANCE → bearish aggression → SHORT

use super::bars::Bar;
use super::impulse::ImpulseDirection;
use super::lvn::LvnLevel;
use super::market_state::{detect_market_state, MarketState, MarketStateConfig};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Configuration for LVN retest strategy
#[derive(Debug, Clone)]
pub struct LvnRetestConfig {
    /// Tolerance for "at level" detection (points)
    pub level_tolerance: f64,
    /// Distance price must move away before level is "armed" (points)
    pub retest_distance: f64,
    /// Minimum delta magnitude for absorption signal
    pub min_delta_for_absorption: i64,
    /// Maximum range for absorption (price didn't move despite delta)
    pub max_range_for_absorption: f64,
    /// Stop loss in points
    pub stop_loss: f64,
    /// Take profit in points
    pub take_profit: f64,
    /// Trailing stop distance (activates at breakeven + this amount)
    pub trailing_stop: f64,
    /// Maximum hold time in bars (1-second bars)
    pub max_hold_bars: usize,
    /// Only trade during RTH
    pub rth_only: bool,
    /// Minimum bars between trades (global cooldown)
    pub cooldown_bars: usize,
    /// Cooldown per level after trading it
    pub level_cooldown_bars: usize,
    /// Maximum volume ratio for LVN quality (lower = thinner = better)
    pub max_lvn_volume_ratio: f64,
    /// Only use same-day LVNs (freshness filter)
    pub same_day_only: bool,
    /// Require multiple bars of absorption (consecutive bars holding)
    pub min_absorption_bars: usize,
    /// Structure-based stop buffer (points beyond the LVN level)
    pub structure_stop_buffer: f64,
    /// Trading start hour (ET, 24h format, e.g. 9 for 9:00 AM)
    pub trade_start_hour: u32,
    /// Trading start minute (e.g. 30 for 9:30)
    pub trade_start_minute: u32,
    /// Trading end hour (ET, 24h format, e.g. 12 for 12:00 PM)
    pub trade_end_hour: u32,
    /// Trading end minute
    pub trade_end_minute: u32,
}

impl Default for LvnRetestConfig {
    fn default() -> Self {
        Self {
            level_tolerance: 2.0,      // Within 2 points of LVN
            retest_distance: 8.0,       // Must move 8+ points away to arm
            min_delta_for_absorption: 100, // 100+ contracts delta
            max_range_for_absorption: 1.5, // Range < 1.5 points = absorbed
            stop_loss: 4.0,
            take_profit: 8.0,
            trailing_stop: 4.0,
            max_hold_bars: 300,         // 5 minutes max
            rth_only: true,
            cooldown_bars: 60,          // 1 minute between trades
            level_cooldown_bars: 600,   // 10 minutes per level
            max_lvn_volume_ratio: 0.15, // Default: any valid LVN
            same_day_only: false,       // Default: use all LVNs
            min_absorption_bars: 1,     // Default: single bar signal
            structure_stop_buffer: 2.0, // Default: 2 pts beyond level
            trade_start_hour: 9,        // 9:30 AM ET
            trade_start_minute: 30,
            trade_end_hour: 16,         // 4:00 PM ET (full RTH)
            trade_end_minute: 0,
        }
    }
}

/// State of an LVN level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelState {
    /// Never touched today
    Untouched,
    /// Price has touched but not moved away yet
    Touched,
    /// Price moved away - level is now "armed" for retest
    Armed,
    /// Price is retesting - look for signals
    Retesting,
}

/// Tracked LVN with state
#[derive(Debug, Clone)]
pub struct TrackedLevel {
    pub price: f64,
    pub state: LevelState,
    pub first_touch_bar: Option<usize>,
    pub armed_bar: Option<usize>,
    pub last_traded_bar: Option<usize>,
    /// Track which side price came from (above = true, below = false)
    pub approached_from_above: Option<bool>,
    /// Direction of the impulse that created this LVN
    pub impulse_direction: ImpulseDirection,
    /// Volume ratio - lower = thinner LVN = higher quality
    pub volume_ratio: f64,
    /// Date the LVN was created
    pub lvn_date: chrono::NaiveDate,
    /// Bar index when absorption was detected (counter-pressure absorbed)
    pub absorption_bar: Option<usize>,
    /// Optional: Links to the parent impulse for grouped clearing
    pub impulse_id: Option<Uuid>,
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Long,
    Short,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Long => write!(f, "Long"),
            Direction::Short => write!(f, "Short"),
        }
    }
}

/// Signal emitted when a valid LVN retest is detected
#[derive(Debug, Clone)]
pub struct LvnSignal {
    pub direction: Direction,
    pub price: f64,
    pub level_price: f64,
    pub delta: i64,
    pub reason: String,
}

/// Reusable signal generator for LVN retest strategy
///
/// This struct encapsulates the signal detection logic from the backtester
/// so it can be reused by the live trading engine.
pub struct LvnSignalGenerator {
    config: LvnRetestConfig,
    tracked_levels: BTreeMap<i64, TrackedLevel>,
    bar_count: usize,
    last_trade_bar: Option<usize>,
    bars_buffer: Vec<Bar>,  // Rolling buffer for market state detection
}

impl LvnSignalGenerator {
    /// Create a new signal generator with the given config
    pub fn new(config: LvnRetestConfig) -> Self {
        Self {
            config,
            tracked_levels: BTreeMap::new(),
            bar_count: 0,
            last_trade_bar: None,
            bars_buffer: Vec::with_capacity(200),
        }
    }

    /// Add LVN levels to track
    pub fn add_lvn_levels(&mut self, levels: &[LvnLevel]) {
        for lvn in levels {
            // Quality filter: only use thin LVNs
            if lvn.volume_ratio > self.config.max_lvn_volume_ratio {
                continue;
            }

            let key = (lvn.price * 10.0) as i64;
            self.tracked_levels.insert(key, TrackedLevel {
                price: lvn.price,
                state: LevelState::Untouched,
                first_touch_bar: None,
                armed_bar: None,
                last_traded_bar: None,
                approached_from_above: None,
                impulse_direction: lvn.impulse_direction,
                volume_ratio: lvn.volume_ratio,
                lvn_date: lvn.date,
                absorption_bar: None,
                impulse_id: Some(lvn.impulse_id),
            });
        }
    }

    /// Add LVN levels with explicit impulse ID (for state machine mode)
    /// Returns the number of levels actually added (after quality filtering)
    pub fn add_lvn_levels_with_impulse(&mut self, levels: &[LvnLevel], impulse_id: Uuid) -> usize {
        let mut added = 0;
        for lvn in levels {
            // Quality filter: only use thin LVNs
            if lvn.volume_ratio > self.config.max_lvn_volume_ratio {
                continue;
            }

            let key = (lvn.price * 10.0) as i64;
            self.tracked_levels.insert(key, TrackedLevel {
                price: lvn.price,
                state: LevelState::Untouched,
                first_touch_bar: None,
                armed_bar: None,
                last_traded_bar: None,
                approached_from_above: None,
                impulse_direction: lvn.impulse_direction,
                volume_ratio: lvn.volume_ratio,
                lvn_date: lvn.date,
                absorption_bar: None,
                impulse_id: Some(impulse_id),
            });
            added += 1;
        }
        added
    }

    /// Clear all LVNs that belong to a specific impulse
    pub fn clear_impulse_lvns(&mut self, impulse_id: Uuid) {
        self.tracked_levels.retain(|_, level| {
            level.impulse_id != Some(impulse_id)
        });
    }

    /// Get the impulse ID for a specific level key
    pub fn get_level_impulse_id(&self, level_key: i64) -> Option<Uuid> {
        self.tracked_levels.get(&level_key).and_then(|l| l.impulse_id)
    }

    /// Clear all tracked levels (call at end of day)
    pub fn clear_levels(&mut self) {
        self.tracked_levels.clear();
        self.bar_count = 0;
        self.last_trade_bar = None;
        self.bars_buffer.clear();
    }

    /// Process a bar and check for signals
    /// Returns Some(LvnSignal) if a valid signal is detected
    pub fn process_bar(&mut self, bar: &Bar) -> Option<LvnSignal> {
        self.bar_count += 1;

        // Add bar to buffer for market state detection
        self.bars_buffer.push(bar.clone());
        if self.bars_buffer.len() > 200 {
            self.bars_buffer.remove(0);
        }

        // Need at least 2 bars
        if self.bars_buffer.len() < 2 {
            return None;
        }

        let bar_idx = self.bar_count;
        // Clone prev_bar to avoid borrow issues
        let prev_bar = self.bars_buffer[self.bars_buffer.len() - 2].clone();

        // Debug: Log level states periodically (every 60 bars = 1 minute)
        if !self.tracked_levels.is_empty() && self.bar_count % 60 == 0 {
            let mut states: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            let mut closest_level: Option<(f64, &str, f64)> = None;
            for level in self.tracked_levels.values() {
                let state_name = match level.state {
                    LevelState::Untouched => "untouched",
                    LevelState::Touched => "touched",
                    LevelState::Armed => "armed",
                    LevelState::Retesting => "retesting",
                };
                *states.entry(state_name).or_insert(0) += 1;
                let dist = (bar.close - level.price).abs();
                if closest_level.is_none() || dist < closest_level.as_ref().unwrap().2 {
                    closest_level = Some((level.price, state_name, dist));
                }
            }
            if let Some((lvn_price, state, dist)) = closest_level {
                tracing::info!(
                    "LEVELS: {} total | price={:.2} | closest LVN={:.2} ({}) dist={:.2} | states: {:?}",
                    self.tracked_levels.len(),
                    bar.close,
                    lvn_price,
                    state,
                    dist,
                    states
                );
            }
        }

        // ALWAYS update level states first (so levels transition even outside RTH)
        self.update_level_states(bar_idx, bar, &prev_bar);

        // Check global cooldown
        if let Some(last_bar) = self.last_trade_bar {
            if bar_idx < last_bar + self.config.cooldown_bars {
                return None;
            }
        }

        // Check trading hours (for signal generation only, not level state updates)
        if self.config.rth_only && !self.is_trading_hours(bar) {
            return None;
        }

        // Check for signal
        if let Some((level_key, direction, reason)) = self.check_for_signal(bar_idx, bar) {
            // Check level cooldown
            if let Some(level) = self.tracked_levels.get(&level_key) {
                if let Some(last_bar) = level.last_traded_bar {
                    if bar_idx < last_bar + self.config.level_cooldown_bars {
                        return None;
                    }
                }
            }

            // Mark as traded
            self.last_trade_bar = Some(bar_idx);
            if let Some(level) = self.tracked_levels.get_mut(&level_key) {
                level.last_traded_bar = Some(bar_idx);
                level.state = LevelState::Touched;
            }

            let level_price = level_key as f64 / 10.0;
            return Some(LvnSignal {
                direction,
                price: bar.close,
                level_price,
                delta: bar.delta,
                reason,
            });
        }

        None
    }

    /// Update the state of all tracked levels based on current price
    fn update_level_states(&mut self, bar_idx: usize, bar: &Bar, prev_bar: &Bar) {
        let price = bar.close;
        let prev_price = prev_bar.close;

        for level in self.tracked_levels.values_mut() {
            let distance = (price - level.price).abs();

            match level.state {
                LevelState::Untouched => {
                    if distance <= self.config.level_tolerance {
                        level.state = LevelState::Touched;
                        level.first_touch_bar = Some(bar_idx);
                        level.approached_from_above = Some(prev_price > level.price);
                    }
                }
                LevelState::Touched => {
                    if distance > self.config.retest_distance {
                        level.state = LevelState::Armed;
                        level.armed_bar = Some(bar_idx);
                    }
                }
                LevelState::Armed => {
                    if distance <= self.config.level_tolerance {
                        level.state = LevelState::Retesting;
                        level.approached_from_above = Some(prev_price > level.price);
                    }
                }
                LevelState::Retesting => {
                    if distance > self.config.level_tolerance * 2.0 {
                        if distance > self.config.retest_distance {
                            level.state = LevelState::Armed;
                        } else {
                            level.state = LevelState::Touched;
                        }
                    }
                }
            }
        }
    }

    /// Check for trend continuation signal at retesting LVN
    fn check_for_signal(&self, _bar_idx: usize, bar: &Bar) -> Option<(i64, Direction, String)> {
        // First check if we have any retesting levels
        let retesting_count = self.tracked_levels.values()
            .filter(|l| l.state == LevelState::Retesting)
            .count();

        // ELEMENT 1: Market State - must be IMBALANCED (trending)
        let market_config = MarketStateConfig::default();
        let market_state = detect_market_state(&self.bars_buffer, self.bars_buffer.len() - 1, &market_config);

        // Log why we're not generating signals (only when there are retesting levels)
        if retesting_count > 0 && self.bar_count % 60 == 0 {
            tracing::info!(
                "SIGNAL CHECK: {} retesting | market={:?} | delta={} (need {}) | price={:.2}",
                retesting_count,
                market_state.state,
                bar.delta,
                self.config.min_delta_for_absorption,
                bar.close
            );
        }

        if market_state.state != MarketState::Imbalanced {
            return None;
        }

        let price = bar.close;
        let delta = bar.delta;
        let range = bar.high - bar.low;

        // ELEMENT 3: HEAVY aggression in trend direction
        if delta.abs() < self.config.min_delta_for_absorption {
            return None;
        }

        let current_date = bar.timestamp.date_naive();

        // ELEMENT 2: Location - find a retesting level near current price
        for (&key, level) in self.tracked_levels.iter() {
            if level.state != LevelState::Retesting {
                continue;
            }

            if self.config.same_day_only && level.lvn_date != current_date {
                continue;
            }

            let distance = (price - level.price).abs();
            if distance > self.config.level_tolerance {
                continue;
            }

            // Trade direction based on impulse direction
            let trade_direction = match level.impulse_direction {
                ImpulseDirection::Up => Direction::Long,
                ImpulseDirection::Down => Direction::Short,
            };

            // TREND CONTINUATION: HEAVY aggression IN the trend direction
            let is_trend_continuation = match trade_direction {
                Direction::Long => delta > 0,
                Direction::Short => delta < 0,
            };

            if !is_trend_continuation {
                continue;
            }

            // Price should hold (not break through the level)
            if range > self.config.max_range_for_absorption {
                continue;
            }

            let reason = format!(
                "Trend continuation at LVN {:.2}: impulse={:?}, delta={}, range={:.2}",
                level.price, level.impulse_direction, delta, range
            );

            return Some((key, trade_direction, reason));
        }

        None
    }

    /// Check if bar is during configured trading hours
    fn is_trading_hours(&self, bar: &Bar) -> bool {
        // Use proper timezone conversion (handles DST correctly)
        use chrono_tz::America::New_York;
        let et_time = bar.timestamp.with_timezone(&New_York);
        let hour = et_time.hour();
        let minute = et_time.minute();
        let time_mins = hour * 60 + minute;

        let start_mins = self.config.trade_start_hour * 60 + self.config.trade_start_minute;
        let end_mins = self.config.trade_end_hour * 60 + self.config.trade_end_minute;

        time_mins >= start_mins && time_mins < end_mins
    }

    /// Get the current config
    pub fn config(&self) -> &LvnRetestConfig {
        &self.config
    }

    /// Get tracked level count
    pub fn level_count(&self) -> usize {
        self.tracked_levels.len()
    }
}
