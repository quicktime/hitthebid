//! Three-Element Backtesting System
//!
//! Generates trades only when ALL THREE elements align:
//! 1. Market State (Balanced vs Imbalanced)
//! 2. Location (at key levels)
//! 3. Aggression (CVD momentum, stacked imbalances, trade flow)

use crate::bars::Bar;
use crate::levels::{compute_daily_levels, DailyLevels, KeyLevel, LevelIndex, LevelType};
use crate::lvn::LvnLevel;
use crate::market_state::{
    precompute_market_states, MarketState, MarketStateConfig, MarketStateResult,
};
use crate::replay::CapturedSignal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trade model determines entry/exit parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeModel {
    /// Balanced market: fade at extremes, tighter stops
    MeanReversion,
    /// Imbalanced market: join trend, wider stops with trailing
    TrendContinuation,
}

impl std::fmt::Display for TradeModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeModel::MeanReversion => write!(f, "Mean Reversion"),
            TradeModel::TrendContinuation => write!(f, "Trend Continuation"),
        }
    }
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeDirection {
    Long,
    Short,
}

impl std::fmt::Display for TradeDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeDirection::Long => write!(f, "LONG"),
            TradeDirection::Short => write!(f, "SHORT"),
        }
    }
}

/// Trade outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeOutcome {
    Win,
    Loss,
    Timeout, // Max hold time exceeded
}

/// Type of aggression detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggressionType {
    /// Strong increase in CVD (buying pressure accelerating)
    DeltaMomentumUp,
    /// Strong decrease in CVD (selling pressure accelerating)
    DeltaMomentumDown,
    /// Stacked bid imbalances (aggressive buying at multiple levels)
    StackedBidImbalance,
    /// Stacked ask imbalances (aggressive selling at multiple levels)
    StackedAskImbalance,
    /// High trade imbalance ratio in current bar
    TradeFlowImbalance,
    /// From captured signal (delta_flip, absorption, stacked_imbalance)
    CapturedSignal,
}

impl std::fmt::Display for AggressionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggressionType::DeltaMomentumUp => write!(f, "Delta Momentum Up"),
            AggressionType::DeltaMomentumDown => write!(f, "Delta Momentum Down"),
            AggressionType::StackedBidImbalance => write!(f, "Stacked Bid Imbalance"),
            AggressionType::StackedAskImbalance => write!(f, "Stacked Ask Imbalance"),
            AggressionType::TradeFlowImbalance => write!(f, "Trade Flow Imbalance"),
            AggressionType::CapturedSignal => write!(f, "Signal"),
        }
    }
}

/// Detected aggression event
#[derive(Debug, Clone)]
pub struct AggressionEvent {
    pub bar_idx: usize,
    pub timestamp: DateTime<Utc>,
    pub aggression_type: AggressionType,
    pub direction: TradeDirection, // Bullish = Long, Bearish = Short
    pub strength: f64,             // 0.0 - 1.0, higher = stronger
    pub price: f64,
    pub delta_change: i64, // CVD change that triggered this
}

/// Configuration for aggression detection
#[derive(Debug, Clone)]
pub struct AggressionConfig {
    /// Number of bars to look back for averages and delta momentum
    pub lookback: usize,
    /// Minimum delta change to trigger momentum signal (absolute)
    pub delta_momentum_threshold: i64,
    /// Minimum imbalance ratio (buy_vol/sell_vol or vice versa) for trade flow
    pub imbalance_ratio_threshold: f64,
    /// Volume spike multiplier (volume > N * average = spike)
    pub volume_spike_mult: f64,
    /// Minimum volume for a bar to be considered (filter noise)
    pub min_volume: u64,
    /// Use captured signals (delta_flip, absorption, stacked_imbalance)
    pub use_captured_signals: bool,
}

impl Default for AggressionConfig {
    fn default() -> Self {
        Self {
            lookback: 60,                    // 60 seconds lookback for averages
            delta_momentum_threshold: 500,   // 500 contracts delta change (strong momentum)
            imbalance_ratio_threshold: 3.0,  // 3:1 buy:sell or sell:buy (heavy imbalance)
            volume_spike_mult: 5.0,          // 5x average volume = significant spike
            min_volume: 50,                  // Minimum 50 contracts (filter noise)
            use_captured_signals: true,
        }
    }
}

/// Configuration for the three-element backtester
#[derive(Debug, Clone)]
pub struct ThreeElementConfig {
    // Market State parameters
    pub market_state: MarketStateConfig,

    // Aggression parameters
    pub aggression: AggressionConfig,

    // Level proximity
    pub level_tolerance: f64, // Points for "at level" (default: 2.0)

    // Mean Reversion trade parameters
    pub mr_stop_loss: f64,     // Points (default: 6.0)
    pub mr_take_profit: f64,   // Points (default: 12.0)
    pub mr_max_hold_bars: u32, // Max bars to hold (default: 3)

    // Trend Continuation trade parameters
    pub tc_stop_loss: f64,     // Points (default: 10.0)
    pub tc_take_profit: f64,   // Points (default: 30.0)
    pub tc_max_hold_bars: u32, // Max bars to hold (default: 10)
    pub tc_trailing_stop: f64, // Points trailing (default: 5.0)

    // Filtering
    pub rth_only: bool, // Only trade Regular Trading Hours

    // Cooldowns (in 1-second bars)
    pub global_cooldown: usize,  // Minimum seconds between any trades
    pub level_cooldown: usize,   // Minimum seconds before re-trading same level
}

impl Default for ThreeElementConfig {
    fn default() -> Self {
        Self {
            market_state: MarketStateConfig::default(),
            aggression: AggressionConfig::default(),
            level_tolerance: 1.0,
            mr_stop_loss: 6.0,
            mr_take_profit: 12.0,
            mr_max_hold_bars: 180,  // 3 minutes in 1-second bars
            tc_stop_loss: 10.0,
            tc_take_profit: 30.0,
            tc_max_hold_bars: 600,  // 10 minutes in 1-second bars
            tc_trailing_stop: 5.0,
            rth_only: true,
            global_cooldown: 600,   // 10 minutes between trades
            level_cooldown: 1800,   // 30 minutes per level
        }
    }
}

/// A generated trade from three-element alignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeElementTrade {
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub direction: TradeDirection,
    pub model: TradeModel,
    pub entry_price: f64,
    pub exit_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub pnl_points: f64,
    pub outcome: TradeOutcome,

    // Context - what triggered this trade
    pub aggression_type: String,
    pub level_type: LevelType,
    pub level_price: f64,
    pub market_state: MarketState,
    pub delta_change: i64,
    pub trend_direction: i8,
}

/// Statistics for a subset of trades
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelStats {
    pub trade_count: u32,
    pub wins: u32,
    pub losses: u32,
    pub timeouts: u32,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
}

impl ModelStats {
    pub fn from_trades(trades: &[&ThreeElementTrade]) -> Self {
        if trades.is_empty() {
            return Self::default();
        }

        let mut stats = Self {
            trade_count: trades.len() as u32,
            ..Default::default()
        };

        let mut total_wins = 0.0;
        let mut total_losses = 0.0;

        for trade in trades {
            match trade.outcome {
                TradeOutcome::Win => {
                    stats.wins += 1;
                    total_wins += trade.pnl_points;
                    if trade.pnl_points > stats.largest_win {
                        stats.largest_win = trade.pnl_points;
                    }
                }
                TradeOutcome::Loss => {
                    stats.losses += 1;
                    total_losses += trade.pnl_points.abs();
                    if trade.pnl_points.abs() > stats.largest_loss {
                        stats.largest_loss = trade.pnl_points.abs();
                    }
                }
                TradeOutcome::Timeout => {
                    stats.timeouts += 1;
                    if trade.pnl_points >= 0.0 {
                        total_wins += trade.pnl_points;
                    } else {
                        total_losses += trade.pnl_points.abs();
                    }
                }
            }
            stats.total_pnl += trade.pnl_points;
        }

        stats.win_rate = if stats.trade_count > 0 {
            stats.wins as f64 / stats.trade_count as f64 * 100.0
        } else {
            0.0
        };

        stats.avg_win = if stats.wins > 0 {
            total_wins / stats.wins as f64
        } else {
            0.0
        };

        stats.avg_loss = if stats.losses > 0 {
            total_losses / stats.losses as f64
        } else {
            0.0
        };

        stats.profit_factor = if total_losses > 0.0 {
            total_wins / total_losses
        } else if total_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        stats
    }
}

/// Complete backtest results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeElementResults {
    pub config: String, // Serialized config summary

    // Overall stats
    pub total_trades: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub total_pnl: f64,

    // By model
    pub mean_reversion_stats: ModelStats,
    pub trend_continuation_stats: ModelStats,

    // By level type
    pub stats_by_level: HashMap<String, ModelStats>,

    // By aggression type
    pub stats_by_aggression: HashMap<String, ModelStats>,

    // All trades for detailed analysis
    pub trades: Vec<ThreeElementTrade>,
}

/// The Three-Element Backtester
pub struct ThreeElementBacktester {
    bars: Vec<Bar>,
    signals: Vec<CapturedSignal>,
    daily_levels: Vec<DailyLevels>,
    lvn_levels: Vec<LvnLevel>,
    level_index: LevelIndex,
    market_states: Vec<MarketStateResult>,
    config: ThreeElementConfig,
}

impl ThreeElementBacktester {
    /// Create a new backtester from bars and signals
    pub fn new(
        bars: Vec<Bar>,
        signals: Vec<CapturedSignal>,
        lvn_levels: Vec<LvnLevel>,
        config: ThreeElementConfig,
    ) -> Self {
        // Compute daily levels from bars
        let daily_levels = compute_daily_levels(&bars);

        // Build level index
        let level_index = LevelIndex::new(&daily_levels, &lvn_levels, config.level_tolerance);

        // Pre-compute market states for all bars
        let market_states = precompute_market_states(&bars, &config.market_state);

        Self {
            bars,
            signals,
            daily_levels,
            lvn_levels,
            level_index,
            market_states,
            config,
        }
    }

    /// Detect aggression events from bars (heavy volume, delta spikes, imbalances)
    /// This mimics what you visually see with "bubbles" - large buying/selling pressure
    fn detect_aggression_from_bars(&self) -> Vec<AggressionEvent> {
        let mut events = Vec::new();
        let lookback = self.config.aggression.lookback;

        if self.bars.len() < lookback + 1 {
            return events;
        }

        for i in lookback..self.bars.len() {
            let bar = &self.bars[i];

            // Skip low-volume bars (noise)
            if bar.volume < self.config.aggression.min_volume {
                continue;
            }

            // Calculate rolling averages for volume and delta
            let window = &self.bars[i.saturating_sub(lookback)..i];
            if window.is_empty() {
                continue;
            }

            let avg_volume: f64 = window.iter().map(|b| b.volume as f64).sum::<f64>() / window.len() as f64;
            let avg_delta_magnitude: f64 = window.iter().map(|b| b.delta.abs() as f64).sum::<f64>() / window.len() as f64;

            // ===== VOLUME SPIKE DETECTION =====
            // Heavy buying/selling = volume significantly above average
            let volume_spike = bar.volume as f64 > avg_volume * self.config.aggression.volume_spike_mult;

            // ===== DELTA SPIKE DETECTION =====
            // Strong directional aggression = delta magnitude way above average
            let delta_spike = bar.delta.abs() as f64 > avg_delta_magnitude * self.config.aggression.volume_spike_mult;

            // ===== IMBALANCE DETECTION =====
            let buy_vol = bar.buy_volume as f64;
            let sell_vol = bar.sell_volume as f64;
            let total_vol = buy_vol + sell_vol;

            // Convert ratio threshold to percentage (3:1 = 75%, 4:1 = 80%, etc.)
            let imbalance_pct = self.config.aggression.imbalance_ratio_threshold
                / (self.config.aggression.imbalance_ratio_threshold + 1.0);

            let (has_imbalance, imbalance_direction) = if total_vol > 0.0 {
                let buy_pct = buy_vol / total_vol;
                let sell_pct = sell_vol / total_vol;

                // Use configurable imbalance threshold
                if buy_pct >= imbalance_pct {
                    (true, TradeDirection::Long)
                } else if sell_pct >= imbalance_pct {
                    (true, TradeDirection::Short)
                } else {
                    (false, TradeDirection::Long)
                }
            } else {
                (false, TradeDirection::Long)
            };

            // ===== GENERATE AGGRESSION EVENTS =====

            // Volume spike with clear imbalance = HEAVY buying/selling (bubble)
            if volume_spike && has_imbalance {
                let strength = (bar.volume as f64 / avg_volume / self.config.aggression.volume_spike_mult).min(1.0);
                events.push(AggressionEvent {
                    bar_idx: i,
                    timestamp: bar.timestamp,
                    aggression_type: AggressionType::TradeFlowImbalance,
                    direction: imbalance_direction,
                    strength,
                    price: bar.close,
                    delta_change: bar.delta,
                });
            }

            // Delta spike = strong directional pressure (momentum)
            if delta_spike {
                let direction = if bar.delta > 0 {
                    TradeDirection::Long
                } else {
                    TradeDirection::Short
                };
                let strength = (bar.delta.abs() as f64 / avg_delta_magnitude / self.config.aggression.volume_spike_mult).min(1.0);

                events.push(AggressionEvent {
                    bar_idx: i,
                    timestamp: bar.timestamp,
                    aggression_type: if direction == TradeDirection::Long {
                        AggressionType::DeltaMomentumUp
                    } else {
                        AggressionType::DeltaMomentumDown
                    },
                    direction,
                    strength,
                    price: bar.close,
                    delta_change: bar.delta,
                });
            }

            // Also check cumulative delta momentum (CVD acceleration)
            let delta_sum: i64 = self.bars[i.saturating_sub(3)..=i].iter().map(|b| b.delta).sum();
            if delta_sum.abs() >= self.config.aggression.delta_momentum_threshold {
                let direction = if delta_sum > 0 {
                    TradeDirection::Long
                } else {
                    TradeDirection::Short
                };
                let strength = (delta_sum.abs() as f64 / self.config.aggression.delta_momentum_threshold as f64).min(1.0);

                events.push(AggressionEvent {
                    bar_idx: i,
                    timestamp: bar.timestamp,
                    aggression_type: if direction == TradeDirection::Long {
                        AggressionType::DeltaMomentumUp
                    } else {
                        AggressionType::DeltaMomentumDown
                    },
                    direction,
                    strength,
                    price: bar.close,
                    delta_change: delta_sum,
                });
            }
        }

        events
    }

    /// Convert captured signals to aggression events
    fn signals_to_aggression_events(&self) -> Vec<AggressionEvent> {
        let mut events = Vec::new();

        for signal in &self.signals {
            let signal_time = match DateTime::from_timestamp_millis(signal.timestamp as i64) {
                Some(t) => t,
                None => continue,
            };

            let bar_idx = match self.find_bar_index_binary(signal_time) {
                Some(idx) => idx,
                None => continue,
            };

            let direction = match signal.direction.as_str() {
                "bullish" => TradeDirection::Long,
                "bearish" => TradeDirection::Short,
                _ => continue,
            };

            // Determine aggression type from signal type
            let aggression_type = match signal.signal_type.as_str() {
                "stacked_imbalance" => {
                    if direction == TradeDirection::Long {
                        AggressionType::StackedBidImbalance
                    } else {
                        AggressionType::StackedAskImbalance
                    }
                }
                _ => AggressionType::CapturedSignal,
            };

            let price = if signal.price > 0.0 {
                signal.price
            } else {
                self.bars.get(bar_idx).map(|b| b.close).unwrap_or(0.0)
            };

            // Parse delta change from extra_data if available
            let delta_change = signal
                .extra_data
                .as_ref()
                .and_then(|d: &String| {
                    // Try to parse "cvd: X -> Y" format
                    if let Some(arrow_pos) = d.find("->") {
                        d[arrow_pos + 2..].trim().parse::<i64>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            events.push(AggressionEvent {
                bar_idx,
                timestamp: signal_time,
                aggression_type,
                direction,
                strength: 0.8, // Default strength for signals
                price,
                delta_change,
            });
        }

        events
    }

    /// Find bar index using binary search (bars should be sorted by timestamp)
    fn find_bar_index_binary(&self, timestamp: DateTime<Utc>) -> Option<usize> {
        if self.bars.is_empty() {
            return None;
        }

        let mut low = 0;
        let mut high = self.bars.len();

        while low < high {
            let mid = (low + high) / 2;
            if self.bars[mid].timestamp <= timestamp {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        if low > 0 {
            Some(low - 1)
        } else if !self.bars.is_empty() && self.bars[0].timestamp <= timestamp {
            Some(0)
        } else {
            None
        }
    }

    /// Check if timestamp is in active trading windows (EST):
    /// - Morning session: 9:30 AM - 12:00 PM EST (14:30 - 17:00 UTC)
    /// - Power hour: 3:00 PM - 4:00 PM EST (20:00 - 21:00 UTC)
    fn is_rth(&self, timestamp: DateTime<Utc>) -> bool {
        use chrono::Timelike;
        let hour = timestamp.hour();
        let minute = timestamp.minute();
        let time_mins = hour * 60 + minute;

        // Morning session: 14:30 - 17:00 UTC (9:30 AM - 12:00 PM EST)
        let morning_start = 14 * 60 + 30;  // 14:30 UTC
        let morning_end = 17 * 60;          // 17:00 UTC

        // Power hour: 20:00 - 21:00 UTC (3:00 PM - 4:00 PM EST)
        let power_start = 20 * 60;          // 20:00 UTC
        let power_end = 21 * 60;            // 21:00 UTC

        (time_mins >= morning_start && time_mins < morning_end)
            || (time_mins >= power_start && time_mins < power_end)
    }

    /// Determine if aggression aligns with level and market state
    fn should_take_trade(
        &self,
        event: &AggressionEvent,
        level: &KeyLevel,
        market_state: &MarketStateResult,
    ) -> Option<TradeDirection> {
        match market_state.state {
            MarketState::Balanced => {
                // Mean Reversion: fade at extremes
                match level.level_type {
                    // Support levels - look for bullish aggression to go long
                    LevelType::VAL | LevelType::PDL | LevelType::ONL => {
                        if event.direction == TradeDirection::Long {
                            Some(TradeDirection::Long)
                        } else {
                            None
                        }
                    }
                    // Resistance levels - look for bearish aggression to go short
                    LevelType::VAH | LevelType::PDH | LevelType::ONH => {
                        if event.direction == TradeDirection::Short {
                            Some(TradeDirection::Short)
                        } else {
                            None
                        }
                    }
                    // Neutral levels (POC, LVN) - follow aggression direction
                    LevelType::POC | LevelType::LVN => Some(event.direction),
                }
            }
            MarketState::Imbalanced => {
                // Trend Continuation: only join the trend
                let trend_up = market_state.trend_direction > 0;
                let trend_down = market_state.trend_direction < 0;

                if trend_up && event.direction == TradeDirection::Long {
                    Some(TradeDirection::Long)
                } else if trend_down && event.direction == TradeDirection::Short {
                    Some(TradeDirection::Short)
                } else {
                    None // Don't counter-trend
                }
            }
        }
    }

    /// Simulate trade execution from entry bar
    /// Returns (trade, exit_bar_idx) for proper blocking
    /// NOTE: We enter at the NEXT bar's open (not signal bar's close) to avoid look-ahead bias
    fn simulate_trade(
        &self,
        signal_bar_idx: usize,
        direction: TradeDirection,
        model: TradeModel,
        event: &AggressionEvent,
        level: &KeyLevel,
        market_state: &MarketStateResult,
    ) -> Option<(ThreeElementTrade, usize)> {
        // Entry is at NEXT bar's open (realistic - can't enter instantly on signal)
        let entry_bar_idx = signal_bar_idx + 1;
        if entry_bar_idx >= self.bars.len() {
            return None;
        }

        let entry_bar = &self.bars[entry_bar_idx];
        let entry_price = entry_bar.open; // Enter at open, not close

        let (stop_loss_pts, take_profit_pts, max_hold) = match model {
            TradeModel::MeanReversion => (
                self.config.mr_stop_loss,
                self.config.mr_take_profit,
                self.config.mr_max_hold_bars,
            ),
            TradeModel::TrendContinuation => (
                self.config.tc_stop_loss,
                self.config.tc_take_profit,
                self.config.tc_max_hold_bars,
            ),
        };

        let (stop_loss, take_profit) = match direction {
            TradeDirection::Long => (entry_price - stop_loss_pts, entry_price + take_profit_pts),
            TradeDirection::Short => (entry_price + stop_loss_pts, entry_price - take_profit_pts),
        };

        let mut exit_bar_idx = entry_bar_idx;
        let mut exit_price = entry_price;
        let mut outcome = TradeOutcome::Timeout;
        let mut trailing_stop = stop_loss;
        let mut highest_price = entry_price;
        let mut lowest_price = entry_price;

        for i in (entry_bar_idx + 1)..self.bars.len().min(entry_bar_idx + max_hold as usize + 1) {
            let bar = &self.bars[i];
            exit_bar_idx = i;

            highest_price = highest_price.max(bar.high);
            lowest_price = lowest_price.min(bar.low);

            // Update trailing stop for trend continuation
            // Only activate trailing AFTER price moves trailing_distance in our favor (breakeven protection)
            if model == TradeModel::TrendContinuation {
                let activation_distance = self.config.tc_trailing_stop; // Activate at breakeven
                match direction {
                    TradeDirection::Long => {
                        // Only start trailing after breakeven (entry + trailing_distance)
                        if highest_price >= entry_price + activation_distance {
                            let new_trail = highest_price - self.config.tc_trailing_stop;
                            if new_trail > trailing_stop {
                                trailing_stop = new_trail;
                            }
                        }
                    }
                    TradeDirection::Short => {
                        // Only start trailing after breakeven (entry - trailing_distance)
                        if lowest_price <= entry_price - activation_distance {
                            let new_trail = lowest_price + self.config.tc_trailing_stop;
                            if new_trail < trailing_stop {
                                trailing_stop = new_trail;
                            }
                        }
                    }
                }
            }

            // Check TP first - if price reaches our target, we exit there
            // This is more realistic: we'd have a limit order at TP
            match direction {
                TradeDirection::Long => {
                    if bar.high >= take_profit {
                        exit_price = take_profit;
                        outcome = TradeOutcome::Win;
                        break;
                    }
                    if bar.low <= trailing_stop {
                        exit_price = trailing_stop;
                        outcome = TradeOutcome::Loss;
                        break;
                    }
                }
                TradeDirection::Short => {
                    if bar.low <= take_profit {
                        exit_price = take_profit;
                        outcome = TradeOutcome::Win;
                        break;
                    }
                    if bar.high >= trailing_stop {
                        exit_price = trailing_stop;
                        outcome = TradeOutcome::Loss;
                        break;
                    }
                }
            }
        }

        // If we timed out, use bar close as exit price
        if outcome == TradeOutcome::Timeout {
            exit_price = self.bars[exit_bar_idx].close;
        }

        // Sanity checks for NQ futures
        // 1. Price should be in reasonable NQ range (10000-50000)
        if entry_price < 10000.0 || entry_price > 50000.0 {
            return None;
        }
        // 2. Exit price should be close to entry (no 500+ point moves in a single trade)
        if (exit_price - entry_price).abs() > 500.0 {
            return None;
        }

        let pnl_points = match direction {
            TradeDirection::Long => exit_price - entry_price,
            TradeDirection::Short => entry_price - exit_price,
        };

        // For timeout trades, determine outcome based on P&L
        // Win/Loss from TP/SL hits are already set correctly
        let outcome = match outcome {
            TradeOutcome::Win => TradeOutcome::Win, // TP was hit - keep it
            TradeOutcome::Loss => {
                // SL was hit, but trailing stop might have been profitable
                if pnl_points > 0.0 {
                    TradeOutcome::Timeout // Profitable trailing stop exit
                } else {
                    TradeOutcome::Loss
                }
            }
            TradeOutcome::Timeout => {
                // Timed out - classify by P&L
                if pnl_points > 0.0 {
                    TradeOutcome::Timeout
                } else {
                    TradeOutcome::Loss
                }
            }
        };

        Some((
            ThreeElementTrade {
                entry_time: entry_bar.timestamp,
                exit_time: self.bars[exit_bar_idx].timestamp,
                direction,
                model,
                entry_price,
                exit_price,
                stop_loss,
                take_profit,
                pnl_points,
                outcome,
                aggression_type: event.aggression_type.to_string(),
                level_type: level.level_type,
                level_price: level.price,
                market_state: market_state.state,
                delta_change: event.delta_change,
                trend_direction: market_state.trend_direction,
            },
            exit_bar_idx,  // Return exit bar index for blocking
        ))
    }

    /// Run the backtest
    pub fn run(&self) -> ThreeElementResults {
        let mut trades = Vec::new();

        println!("Running Three-Element Backtest...");
        println!("  Bars: {}", self.bars.len());
        println!("  Captured Signals: {}", self.signals.len());
        println!("  Daily Levels: {}", self.daily_levels.len());
        println!("  LVN Levels: {}", self.lvn_levels.len());
        println!("  Total Key Levels: {}", self.level_index.len());

        // Collect all aggression events
        let mut aggression_events = Vec::new();

        // From bars (CVD momentum, trade flow)
        let bar_events = self.detect_aggression_from_bars();
        println!("  Aggression from bars: {}", bar_events.len());
        aggression_events.extend(bar_events);

        // From captured signals
        if self.config.aggression.use_captured_signals {
            let signal_events = self.signals_to_aggression_events();
            println!("  Aggression from signals: {}", signal_events.len());
            aggression_events.extend(signal_events);
        }

        // Sort by timestamp
        aggression_events.sort_by_key(|e| e.timestamp);

        println!("  Total aggression events: {}", aggression_events.len());

        // Track last trade time to avoid overlapping trades
        let mut last_trade_exit_bar: Option<usize> = None;

        // Track levels that have been traded with cooldown (level_price -> last_exit_bar)
        // We use i64 as key (price * 100 to handle decimals)
        let mut traded_levels: HashMap<i64, usize> = HashMap::new();
        let level_cooldown_bars = self.config.level_cooldown;
        let global_cooldown_bars = self.config.global_cooldown;

        // Process each aggression event
        for event in &aggression_events {
            // Skip if still in a trade OR within global cooldown
            if let Some(exit_bar) = last_trade_exit_bar {
                if event.bar_idx <= exit_bar + global_cooldown_bars {
                    continue;
                }
            }

            // RTH filter
            if self.config.rth_only && !self.is_rth(event.timestamp) {
                continue;
            }

            if event.bar_idx >= self.market_states.len() {
                continue;
            }

            // ELEMENT 1: Get market state
            let market_state = &self.market_states[event.bar_idx];

            // Get trading date for level lookup
            let date = event.timestamp.date_naive();

            // ELEMENT 2: Check if at a key level
            let level = match self.level_index.strongest_level_near(event.price, date) {
                Some(l) => l,
                None => continue, // Not at a key level
            };

            // Check if this level is on cooldown (recently traded)
            let level_key = (level.price * 100.0) as i64;
            if let Some(&last_exit) = traded_levels.get(&level_key) {
                if event.bar_idx < last_exit + level_cooldown_bars {
                    continue; // Level is on cooldown, skip
                }
            }

            // ELEMENT 3: Already have aggression event

            // Determine trade model based on market state
            let model = match market_state.state {
                MarketState::Balanced => TradeModel::MeanReversion,
                MarketState::Imbalanced => TradeModel::TrendContinuation,
            };

            // Check if all three elements align
            let direction = match self.should_take_trade(event, level, market_state) {
                Some(d) => d,
                None => continue,
            };

            // Simulate the trade
            if let Some((trade, exit_bar_idx)) =
                self.simulate_trade(event.bar_idx, direction, model, event, level, market_state)
            {
                // Track exit to avoid overlapping - use exact bar index
                last_trade_exit_bar = Some(exit_bar_idx);

                // Mark this level as traded with cooldown
                traded_levels.insert(level_key, exit_bar_idx);

                trades.push(trade);
            }
        }

        self.calculate_results(trades)
    }

    /// Calculate statistics from trades
    fn calculate_results(&self, trades: Vec<ThreeElementTrade>) -> ThreeElementResults {
        let total_trades = trades.len() as u32;

        if trades.is_empty() {
            return ThreeElementResults {
                config: format!("{:?}", self.config),
                total_trades: 0,
                win_rate: 0.0,
                profit_factor: 0.0,
                sharpe_ratio: 0.0,
                max_drawdown: 0.0,
                total_pnl: 0.0,
                mean_reversion_stats: ModelStats::default(),
                trend_continuation_stats: ModelStats::default(),
                stats_by_level: HashMap::new(),
                stats_by_aggression: HashMap::new(),
                trades: Vec::new(),
            };
        }

        let total_pnl: f64 = trades.iter().map(|t| t.pnl_points).sum();
        let wins = trades
            .iter()
            .filter(|t| t.outcome == TradeOutcome::Win)
            .count();
        let win_rate = wins as f64 / total_trades as f64 * 100.0;

        let gross_profit: f64 = trades
            .iter()
            .filter(|t| t.pnl_points > 0.0)
            .map(|t| t.pnl_points)
            .sum();
        let gross_loss: f64 = trades
            .iter()
            .filter(|t| t.pnl_points < 0.0)
            .map(|t| t.pnl_points.abs())
            .sum();
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let returns: Vec<f64> = trades.iter().map(|t| t.pnl_points).collect();
        let mean_return = total_pnl / total_trades as f64;
        let variance: f64 = returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / (total_trades as f64 - 1.0).max(1.0);
        let std_dev = variance.sqrt();
        let sharpe_ratio = if std_dev > 0.0 {
            mean_return / std_dev * (252.0_f64).sqrt()
        } else {
            0.0
        };

        let mut peak = 0.0;
        let mut max_drawdown = 0.0;
        let mut cumulative = 0.0;

        for trade in &trades {
            cumulative += trade.pnl_points;
            if cumulative > peak {
                peak = cumulative;
            }
            let drawdown = peak - cumulative;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }

        let mr_trades: Vec<_> = trades
            .iter()
            .filter(|t| t.model == TradeModel::MeanReversion)
            .collect();
        let tc_trades: Vec<_> = trades
            .iter()
            .filter(|t| t.model == TradeModel::TrendContinuation)
            .collect();

        let mean_reversion_stats = ModelStats::from_trades(&mr_trades);
        let trend_continuation_stats = ModelStats::from_trades(&tc_trades);

        let mut stats_by_level: HashMap<String, ModelStats> = HashMap::new();
        for level_type in [
            LevelType::POC,
            LevelType::VAH,
            LevelType::VAL,
            LevelType::PDH,
            LevelType::PDL,
            LevelType::ONH,
            LevelType::ONL,
            LevelType::LVN,
        ] {
            let level_trades: Vec<_> = trades
                .iter()
                .filter(|t| t.level_type == level_type)
                .collect();
            if !level_trades.is_empty() {
                stats_by_level
                    .insert(level_type.to_string(), ModelStats::from_trades(&level_trades));
            }
        }

        let mut stats_by_aggression = HashMap::new();
        let agg_types: std::collections::HashSet<_> =
            trades.iter().map(|t| t.aggression_type.clone()).collect();
        for agg_type in agg_types {
            let agg_trades: Vec<_> = trades
                .iter()
                .filter(|t| t.aggression_type == agg_type)
                .collect();
            stats_by_aggression.insert(agg_type, ModelStats::from_trades(&agg_trades));
        }

        ThreeElementResults {
            config: format!(
                "MR: SL={} TP={}, TC: SL={} TP={}, Tolerance={}, Delta Threshold={}",
                self.config.mr_stop_loss,
                self.config.mr_take_profit,
                self.config.tc_stop_loss,
                self.config.tc_take_profit,
                self.config.level_tolerance,
                self.config.aggression.delta_momentum_threshold
            ),
            total_trades,
            win_rate,
            profit_factor,
            sharpe_ratio,
            max_drawdown,
            total_pnl,
            mean_reversion_stats,
            trend_continuation_stats,
            stats_by_level,
            stats_by_aggression,
            trades,
        }
    }
}

/// Print results in a formatted table
pub fn print_results(results: &ThreeElementResults) {
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("             THREE-ELEMENT BACKTEST RESULTS                ");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Config: {}", results.config);
    println!();
    println!("Overall Performance:");
    println!("  Total Trades:    {}", results.total_trades);
    println!("  Win Rate:        {:.1}%", results.win_rate);
    println!("  Profit Factor:   {:.2}", results.profit_factor);
    println!("  Sharpe Ratio:    {:.2}", results.sharpe_ratio);
    println!("  Max Drawdown:    {:.2} pts", results.max_drawdown);
    println!("  Total P&L:       {:.2} pts", results.total_pnl);
    println!();
    println!("By Trade Model:");
    print_model_stats("  Mean Reversion", &results.mean_reversion_stats);
    print_model_stats("  Trend Continuation", &results.trend_continuation_stats);
    println!();
    println!("By Level Type:");
    for (level, stats) in &results.stats_by_level {
        print_model_stats(&format!("  {}", level), stats);
    }
    println!();
    println!("By Aggression Type:");
    for (agg, stats) in &results.stats_by_aggression {
        print_model_stats(&format!("  {}", agg), stats);
    }
    println!("═══════════════════════════════════════════════════════════");
}

fn print_model_stats(label: &str, stats: &ModelStats) {
    if stats.trade_count > 0 {
        println!(
            "{}: {} trades, {:.1}% win rate, {:.2} pts P&L, PF: {:.2}",
            label, stats.trade_count, stats.win_rate, stats.total_pnl, stats.profit_factor
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_stats() {
        let trades = vec![];
        let stats = ModelStats::from_trades(&trades.iter().collect::<Vec<_>>());
        assert_eq!(stats.trade_count, 0);
        assert_eq!(stats.win_rate, 0.0);
    }
}
