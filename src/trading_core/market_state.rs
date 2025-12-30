//! Market State Detection
//!
//! Determines if the market is Balanced (rotational/mean-reverting) or
//! Imbalanced (directional/trending) based on price action analysis.

use super::bars::Bar;
use serde::{Deserialize, Serialize};

/// Market state classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketState {
    /// Rotational market - price oscillating around fair value
    /// Use Mean Reversion trades (fade at extremes)
    Balanced,
    /// Directional market - price pushing toward new fair value
    /// Use Trend Continuation trades (join momentum)
    Imbalanced,
}

/// Configuration for market state detection
#[derive(Debug, Clone)]
pub struct MarketStateConfig {
    /// Number of bars to look back for analysis (default: 20)
    pub lookback_bars: usize,
    /// Minimum rotations through fair value to be "balanced" (default: 3)
    pub rotation_threshold: u32,
    /// Range expansion multiplier for "imbalanced" (default: 2.0)
    /// If current range > atr * this value, market is imbalanced
    pub range_expansion_mult: f64,
    /// Cumulative delta threshold for "imbalanced" (default: 500)
    /// If absolute cumulative delta > this, market is imbalanced
    pub delta_accumulation_threshold: i64,
}

impl Default for MarketStateConfig {
    fn default() -> Self {
        Self {
            lookback_bars: 60,  // 60 seconds with 1-second bars
            rotation_threshold: 3,
            range_expansion_mult: 2.0,
            delta_accumulation_threshold: 200,  // Lower for 1-second bars
        }
    }
}

/// Result of market state analysis for a specific bar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStateResult {
    /// The determined market state
    pub state: MarketState,
    /// Fair value (VWAP or midpoint of range)
    pub fair_value: f64,
    /// Average True Range over lookback period
    pub atr: f64,
    /// Number of times price crossed fair value
    pub rotation_count: u32,
    /// Current range / ATR ratio
    pub range_ratio: f64,
    /// Cumulative delta over lookback period
    pub cumulative_delta: i64,
    /// Trend direction: 1 = up, -1 = down, 0 = neutral
    pub trend_direction: i8,
}

/// Calculate VWAP (Volume Weighted Average Price) for a window of bars
fn calculate_vwap(bars: &[Bar]) -> f64 {
    if bars.is_empty() {
        return 0.0;
    }

    let mut sum_pv = 0.0;
    let mut sum_v = 0u64;

    for bar in bars {
        let typical_price = (bar.high + bar.low + bar.close) / 3.0;
        sum_pv += typical_price * bar.volume as f64;
        sum_v += bar.volume;
    }

    if sum_v == 0 {
        // Fallback to simple midpoint
        let first = bars.first().unwrap();
        let last = bars.last().unwrap();
        return (first.open + last.close) / 2.0;
    }

    sum_pv / sum_v as f64
}

/// Calculate Average True Range
fn calculate_atr(bars: &[Bar]) -> f64 {
    if bars.len() < 2 {
        return if let Some(bar) = bars.first() {
            bar.high - bar.low
        } else {
            0.0
        };
    }

    let mut sum_tr = 0.0;
    let mut prev_close = bars[0].close;

    for bar in bars.iter().skip(1) {
        let tr = (bar.high - bar.low)
            .max((bar.high - prev_close).abs())
            .max((bar.low - prev_close).abs());
        sum_tr += tr;
        prev_close = bar.close;
    }

    sum_tr / (bars.len() - 1) as f64
}

/// Count how many times price crosses through a level
fn count_fair_value_crosses(bars: &[Bar], fair_value: f64) -> u32 {
    if bars.len() < 2 {
        return 0;
    }

    let mut crosses = 0u32;
    let mut prev_above = bars[0].close > fair_value;

    for bar in bars.iter().skip(1) {
        let curr_above = bar.close > fair_value;
        if curr_above != prev_above {
            crosses += 1;
        }
        prev_above = curr_above;
    }

    crosses
}

/// Detect market state for a specific bar
pub fn detect_market_state(
    bars: &[Bar],
    bar_idx: usize,
    config: &MarketStateConfig,
) -> MarketStateResult {
    // Get the window of bars to analyze
    let start = bar_idx.saturating_sub(config.lookback_bars);
    let end = (bar_idx + 1).min(bars.len());
    let window = &bars[start..end];

    if window.is_empty() {
        return MarketStateResult {
            state: MarketState::Balanced,
            fair_value: 0.0,
            atr: 0.0,
            rotation_count: 0,
            range_ratio: 0.0,
            cumulative_delta: 0,
            trend_direction: 0,
        };
    }

    // Calculate metrics
    let fair_value = calculate_vwap(window);
    let atr = calculate_atr(window);
    let rotation_count = count_fair_value_crosses(window, fair_value);

    // Calculate current range
    let window_high = window.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let window_low = window.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let current_range = window_high - window_low;
    let range_ratio = if atr > 0.0 { current_range / atr } else { 0.0 };

    // Calculate cumulative delta
    let cumulative_delta: i64 = window.iter().map(|b| b.delta).sum();

    // Determine trend direction
    let trend_direction = if cumulative_delta > config.delta_accumulation_threshold / 2 {
        1i8 // Up trend
    } else if cumulative_delta < -(config.delta_accumulation_threshold / 2) {
        -1i8 // Down trend
    } else {
        0i8 // Neutral
    };

    // Classification logic:
    // BALANCED: Many rotations through fair value, range contained
    // IMBALANCED: Range expanding or strong delta accumulation
    let state = if range_ratio >= config.range_expansion_mult {
        // Range expanded significantly - trending
        MarketState::Imbalanced
    } else if cumulative_delta.abs() > config.delta_accumulation_threshold {
        // Strong directional flow - trending
        MarketState::Imbalanced
    } else if rotation_count >= config.rotation_threshold {
        // Multiple rotations through fair value - balanced
        MarketState::Balanced
    } else {
        // Default to balanced (more conservative)
        MarketState::Balanced
    };

    MarketStateResult {
        state,
        fair_value,
        atr,
        rotation_count,
        range_ratio,
        cumulative_delta,
        trend_direction,
    }
}

/// Pre-compute market state for all bars
pub fn precompute_market_states(
    bars: &[Bar],
    config: &MarketStateConfig,
) -> Vec<MarketStateResult> {
    bars.iter()
        .enumerate()
        .map(|(i, _)| detect_market_state(bars, i, config))
        .collect()
}
