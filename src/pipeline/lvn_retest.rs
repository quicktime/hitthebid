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

use crate::bars::Bar;
use crate::impulse::ImpulseDirection;
use crate::lvn::LvnLevel;
use crate::market_state::{detect_market_state, MarketState, MarketStateConfig};
use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
enum LevelState {
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
struct TrackedLevel {
    price: f64,
    state: LevelState,
    first_touch_bar: Option<usize>,
    armed_bar: Option<usize>,
    last_traded_bar: Option<usize>,
    /// Track which side price came from (above = true, below = false)
    approached_from_above: Option<bool>,
    /// Direction of the impulse that created this LVN
    impulse_direction: ImpulseDirection,
    /// Volume ratio - lower = thinner LVN = higher quality
    volume_ratio: f64,
    /// Date the LVN was created
    lvn_date: chrono::NaiveDate,
    /// Bar index when absorption was detected (counter-pressure absorbed)
    absorption_bar: Option<usize>,
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

/// Trade outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Win,
    Loss,
    Breakeven,
    Timeout,
}

/// A single trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub direction: Direction,
    pub entry_price: f64,
    pub exit_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub pnl_points: f64,
    pub outcome: Outcome,
    pub level_price: f64,
    pub hold_bars: usize,
    pub entry_reason: String,
}

/// Backtest results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Results {
    pub total_trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub breakevens: u32,
    pub timeouts: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub trades: Vec<Trade>,
}

/// The LVN Retest Backtester
pub struct LvnRetestBacktester {
    bars: Vec<Bar>,
    lvn_levels: Vec<LvnLevel>,
    config: LvnRetestConfig,
}

impl LvnRetestBacktester {
    pub fn new(bars: Vec<Bar>, lvn_levels: Vec<LvnLevel>, config: LvnRetestConfig) -> Self {
        Self {
            bars,
            lvn_levels,
            config,
        }
    }

    /// Run the backtest
    pub fn run(&self) -> Results {
        let mut trades = Vec::new();
        let mut tracked_levels: HashMap<i64, TrackedLevel> = HashMap::new();
        let mut last_trade_exit_bar: Option<usize> = None;

        // Initialize tracked levels from LVN data (filtered by quality)
        for lvn in &self.lvn_levels {
            // Quality filter: only use thin LVNs
            if lvn.volume_ratio > self.config.max_lvn_volume_ratio {
                continue;
            }

            let key = (lvn.price * 10.0) as i64; // Key by price (0.1 precision)
            tracked_levels.insert(key, TrackedLevel {
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
            });
        }

        // Process each bar
        for i in 1..self.bars.len() {
            let bar = &self.bars[i];
            let prev_bar = &self.bars[i - 1];

            // Skip if in cooldown from last trade
            if let Some(exit_bar) = last_trade_exit_bar {
                if i < exit_bar + self.config.cooldown_bars {
                    // Still update level states even during cooldown
                    self.update_level_states(&mut tracked_levels, i, bar, prev_bar);
                    continue;
                }
            }

            // Skip non-RTH if configured
            if self.config.rth_only && !self.is_trading_hours(bar) {
                continue;
            }

            // Update level states
            self.update_level_states(&mut tracked_levels, i, bar, prev_bar);

            // Check for trend continuation signal at retesting levels
            if let Some((level_key, direction, reason)) =
                self.check_for_signal(&mut tracked_levels, i, bar)
            {
                // Check level cooldown
                if let Some(level) = tracked_levels.get(&level_key) {
                    if let Some(last_bar) = level.last_traded_bar {
                        if i < last_bar + self.config.level_cooldown_bars {
                            continue; // Level on cooldown
                        }
                    }
                }

                // Simulate the trade
                if let Some((trade, exit_bar)) = self.simulate_trade(i, direction, level_key, &reason) {
                    last_trade_exit_bar = Some(exit_bar);

                    // Mark level as traded
                    if let Some(level) = tracked_levels.get_mut(&level_key) {
                        level.last_traded_bar = Some(exit_bar);
                        level.state = LevelState::Touched; // Reset to touched
                    }

                    trades.push(trade);
                }
            }
        }

        self.calculate_results(trades)
    }

    /// Update the state of all tracked levels based on current price
    fn update_level_states(
        &self,
        levels: &mut HashMap<i64, TrackedLevel>,
        bar_idx: usize,
        bar: &Bar,
        prev_bar: &Bar,
    ) {
        let price = bar.close;
        let prev_price = prev_bar.close;

        for level in levels.values_mut() {
            let distance = (price - level.price).abs();
            let prev_distance = (prev_price - level.price).abs();

            match level.state {
                LevelState::Untouched => {
                    if distance <= self.config.level_tolerance {
                        level.state = LevelState::Touched;
                        level.first_touch_bar = Some(bar_idx);
                        // Record which side we approached from
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
                        // Update approach direction for retest
                        level.approached_from_above = Some(prev_price > level.price);
                    }
                }
                LevelState::Retesting => {
                    if distance > self.config.level_tolerance * 2.0 {
                        // Price left the retest zone
                        if distance > self.config.retest_distance {
                            level.state = LevelState::Armed; // Re-arm for another retest
                        } else {
                            level.state = LevelState::Touched;
                        }
                    }
                }
            }
        }
    }

    /// Check for TREND CONTINUATION signal at retesting LVN
    ///
    /// THREE ELEMENTS REQUIRED:
    /// 1. Market State: IMBALANCED (trending)
    /// 2. Location: At retesting LVN (from impulse that broke structure)
    /// 3. Aggression: HEAVY buying (at support) or selling (at resistance)
    fn check_for_signal(
        &self,
        levels: &mut HashMap<i64, TrackedLevel>,
        bar_idx: usize,
        bar: &Bar,
    ) -> Option<(i64, Direction, String)> {
        // ELEMENT 1: Market State - must be IMBALANCED (trending)
        let market_config = MarketStateConfig::default();
        let market_state = detect_market_state(&self.bars, bar_idx, &market_config);

        if market_state.state != MarketState::Imbalanced {
            return None; // Only trade in trending markets
        }

        let price = bar.close;
        let delta = bar.delta;
        let range = bar.high - bar.low;

        // ELEMENT 3: HEAVY aggression in trend direction
        let has_heavy_aggression = delta.abs() >= self.config.min_delta_for_absorption;

        if !has_heavy_aggression {
            return None;
        }

        // Get current bar's date for freshness check
        let current_date = bar.timestamp.date_naive();

        // ELEMENT 2: Location - find a retesting level near current price
        for (&key, level) in levels.iter() {
            if level.state != LevelState::Retesting {
                continue;
            }

            // Freshness filter: only same-day LVNs if configured
            if self.config.same_day_only && level.lvn_date != current_date {
                continue;
            }

            let distance = (price - level.price).abs();
            if distance > self.config.level_tolerance {
                continue;
            }

            // Trade direction based on impulse direction
            // Impulse UP → LVN is SUPPORT → LONG
            // Impulse DOWN → LVN is RESISTANCE → SHORT
            let trade_direction = match level.impulse_direction {
                ImpulseDirection::Up => Direction::Long,
                ImpulseDirection::Down => Direction::Short,
            };

            // TREND CONTINUATION: HEAVY aggression IN the trend direction
            let is_trend_continuation = match trade_direction {
                Direction::Long => delta > 0,   // Heavy buying at support
                Direction::Short => delta < 0,  // Heavy selling at resistance
            };

            if !is_trend_continuation {
                continue;
            }

            // Price should hold (not break through the level)
            let price_held = range <= self.config.max_range_for_absorption;

            if !price_held {
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

    /// Simulate a trade from entry bar
    fn simulate_trade(
        &self,
        signal_bar_idx: usize,
        direction: Direction,
        level_key: i64,
        reason: &str,
    ) -> Option<(Trade, usize)> {
        // Enter at next bar's open
        let entry_bar_idx = signal_bar_idx + 1;
        if entry_bar_idx >= self.bars.len() {
            return None;
        }

        let entry_bar = &self.bars[entry_bar_idx];
        let entry_price = entry_bar.open;
        let level_price = level_key as f64 / 10.0;

        // STRUCTURE-BASED STOPS: Place stop just beyond the LVN level
        // Long at support: stop below LVN
        // Short at resistance: stop above LVN
        let structure_stop_buffer = self.config.structure_stop_buffer;
        let (initial_stop, take_profit) = match direction {
            Direction::Long => (
                level_price - structure_stop_buffer, // Stop below LVN support
                entry_price + self.config.take_profit,
            ),
            Direction::Short => (
                level_price + structure_stop_buffer, // Stop above LVN resistance
                entry_price - self.config.take_profit,
            ),
        };

        let mut exit_bar_idx = entry_bar_idx;
        let mut exit_price = entry_price;
        let mut outcome = Outcome::Timeout;
        let mut trailing_stop = initial_stop;
        let mut highest_price = entry_price;
        let mut lowest_price = entry_price;

        // Simulate bar by bar
        let max_bar = (entry_bar_idx + self.config.max_hold_bars).min(self.bars.len());
        for i in (entry_bar_idx + 1)..max_bar {
            let bar = &self.bars[i];
            highest_price = highest_price.max(bar.high);
            lowest_price = lowest_price.min(bar.low);

            // Update trailing stop
            let activation_distance = self.config.trailing_stop;
            match direction {
                Direction::Long => {
                    if highest_price >= entry_price + activation_distance {
                        let new_trail = highest_price - self.config.trailing_stop;
                        if new_trail > trailing_stop {
                            trailing_stop = new_trail;
                        }
                    }
                    // Check stop (trailing or initial)
                    if bar.low <= trailing_stop {
                        exit_bar_idx = i;
                        exit_price = trailing_stop;
                        let profit = exit_price - entry_price;
                        outcome = if profit > 0.5 {
                            Outcome::Win  // Trailing stop locked in profit
                        } else if profit >= -0.5 {
                            Outcome::Breakeven
                        } else {
                            Outcome::Loss
                        };
                        break;
                    }
                    // Check target
                    if bar.high >= take_profit {
                        exit_bar_idx = i;
                        exit_price = take_profit;
                        outcome = Outcome::Win;
                        break;
                    }
                }
                Direction::Short => {
                    if lowest_price <= entry_price - activation_distance {
                        let new_trail = lowest_price + self.config.trailing_stop;
                        if new_trail < trailing_stop {
                            trailing_stop = new_trail;
                        }
                    }
                    // Check stop (trailing or initial)
                    if bar.high >= trailing_stop {
                        exit_bar_idx = i;
                        exit_price = trailing_stop;
                        let profit = entry_price - exit_price;  // Short profit
                        outcome = if profit > 0.5 {
                            Outcome::Win  // Trailing stop locked in profit
                        } else if profit >= -0.5 {
                            Outcome::Breakeven
                        } else {
                            Outcome::Loss
                        };
                        break;
                    }
                    // Check target
                    if bar.low <= take_profit {
                        exit_bar_idx = i;
                        exit_price = take_profit;
                        outcome = Outcome::Win;
                        break;
                    }
                }
            }

            exit_bar_idx = i;
            exit_price = bar.close;
        }

        // Calculate P&L
        let pnl_points = match direction {
            Direction::Long => exit_price - entry_price,
            Direction::Short => entry_price - exit_price,
        };

        // Adjust outcome based on final P&L
        if outcome == Outcome::Timeout {
            outcome = if pnl_points > 0.5 {
                Outcome::Win
            } else if pnl_points < -0.5 {
                Outcome::Loss
            } else {
                Outcome::Breakeven
            };
        }

        let exit_bar = &self.bars[exit_bar_idx];

        Some((
            Trade {
                entry_time: entry_bar.timestamp,
                exit_time: exit_bar.timestamp,
                direction,
                entry_price,
                exit_price,
                stop_loss: initial_stop,
                take_profit,
                pnl_points,
                outcome,
                level_price,
                hold_bars: exit_bar_idx - entry_bar_idx,
                entry_reason: reason.to_string(),
            },
            exit_bar_idx,
        ))
    }

    /// Check if bar is during configured trading hours
    fn is_trading_hours(&self, bar: &Bar) -> bool {
        let hour = bar.timestamp.hour();
        let minute = bar.timestamp.minute();
        let time_mins = hour * 60 + minute;

        // Convert ET to UTC (add 5 hours)
        let start_utc = (self.config.trade_start_hour + 5) * 60 + self.config.trade_start_minute;
        let end_utc = (self.config.trade_end_hour + 5) * 60 + self.config.trade_end_minute;

        time_mins >= start_utc && time_mins < end_utc
    }

    /// Calculate final results
    fn calculate_results(&self, trades: Vec<Trade>) -> Results {
        let total_trades = trades.len() as u32;

        if trades.is_empty() {
            return Results {
                total_trades: 0,
                wins: 0,
                losses: 0,
                breakevens: 0,
                timeouts: 0,
                win_rate: 0.0,
                profit_factor: 0.0,
                total_pnl: 0.0,
                avg_win: 0.0,
                avg_loss: 0.0,
                max_drawdown: 0.0,
                sharpe_ratio: 0.0,
                trades: Vec::new(),
            };
        }

        let wins = trades.iter().filter(|t| t.outcome == Outcome::Win).count() as u32;
        let losses = trades.iter().filter(|t| t.outcome == Outcome::Loss).count() as u32;
        let breakevens = trades.iter().filter(|t| t.outcome == Outcome::Breakeven).count() as u32;
        let timeouts = trades.iter().filter(|t| t.outcome == Outcome::Timeout).count() as u32;

        let total_pnl: f64 = trades.iter().map(|t| t.pnl_points).sum();
        let win_rate = wins as f64 / total_trades as f64 * 100.0;

        let gross_profit: f64 = trades.iter().filter(|t| t.pnl_points > 0.0).map(|t| t.pnl_points).sum();
        let gross_loss: f64 = trades.iter().filter(|t| t.pnl_points < 0.0).map(|t| t.pnl_points.abs()).sum();

        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        let winning_trades: Vec<_> = trades.iter().filter(|t| t.pnl_points > 0.0).collect();
        let losing_trades: Vec<_> = trades.iter().filter(|t| t.pnl_points < 0.0).collect();

        let avg_win = if !winning_trades.is_empty() {
            winning_trades.iter().map(|t| t.pnl_points).sum::<f64>() / winning_trades.len() as f64
        } else {
            0.0
        };

        let avg_loss = if !losing_trades.is_empty() {
            losing_trades.iter().map(|t| t.pnl_points).sum::<f64>() / losing_trades.len() as f64
        } else {
            0.0
        };

        // Calculate max drawdown
        let mut peak = 0.0f64;
        let mut max_dd = 0.0f64;
        let mut equity = 0.0f64;
        for trade in &trades {
            equity += trade.pnl_points;
            peak = peak.max(equity);
            let dd = peak - equity;
            max_dd = max_dd.max(dd);
        }

        // Calculate Sharpe ratio (simplified - daily returns proxy)
        let returns: Vec<f64> = trades.iter().map(|t| t.pnl_points).collect();
        let mean_return = total_pnl / total_trades as f64;
        let variance: f64 = returns.iter().map(|r| (r - mean_return).powi(2)).sum::<f64>() / total_trades as f64;
        let std_dev = variance.sqrt();
        let sharpe_ratio = if std_dev > 0.0 {
            (mean_return / std_dev) * (252.0_f64).sqrt() // Annualized
        } else {
            0.0
        };

        Results {
            total_trades,
            wins,
            losses,
            breakevens,
            timeouts,
            win_rate,
            profit_factor,
            total_pnl,
            avg_win,
            avg_loss,
            max_drawdown: max_dd,
            sharpe_ratio,
            trades,
        }
    }
}

/// Print results in a nice format
pub fn print_results(results: &Results) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("              LVN RETEST STRATEGY RESULTS                   ");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Overall Performance:");
    println!("  Total Trades:    {}", results.total_trades);
    println!("  Wins:            {} ({:.1}%)", results.wins, results.win_rate);
    println!("  Losses:          {}", results.losses);
    println!("  Breakevens:      {}", results.breakevens);
    println!("  Timeouts:        {}", results.timeouts);
    println!();
    println!("  Profit Factor:   {:.2}", results.profit_factor);
    println!("  Sharpe Ratio:    {:.2}", results.sharpe_ratio);
    println!("  Total P&L:       {:.2} pts", results.total_pnl);
    println!("  Max Drawdown:    {:.2} pts", results.max_drawdown);
    println!();
    println!("  Avg Win:         {:.2} pts", results.avg_win);
    println!("  Avg Loss:        {:.2} pts", results.avg_loss);

    let rr_ratio = if results.avg_loss.abs() > 0.01 {
        results.avg_win / results.avg_loss.abs()
    } else {
        0.0
    };
    println!("  R:R Ratio:       {:.2}:1", rr_ratio);

    // Timing analysis
    if !results.trades.is_empty() {
        println!("\n───────────────────────────────────────────────────────────");
        println!("                    TIMING ANALYSIS                         ");
        println!("───────────────────────────────────────────────────────────\n");

        // By hour (convert UTC to ET: subtract 5 hours)
        let mut by_hour: std::collections::HashMap<u32, (u32, u32, f64)> = std::collections::HashMap::new();
        // By day of week
        let mut by_day: std::collections::HashMap<chrono::Weekday, (u32, u32, f64)> = std::collections::HashMap::new();

        for trade in &results.trades {
            // Convert UTC to ET (approximate - doesn't account for DST)
            let et_hour = (trade.entry_time.hour() + 24 - 5) % 24;
            let day = trade.entry_time.weekday();
            let is_win = trade.outcome == Outcome::Win;

            // Update hour stats
            let hour_entry = by_hour.entry(et_hour).or_insert((0, 0, 0.0));
            hour_entry.0 += 1; // total
            if is_win { hour_entry.1 += 1; } // wins
            hour_entry.2 += trade.pnl_points; // P&L

            // Update day stats
            let day_entry = by_day.entry(day).or_insert((0, 0, 0.0));
            day_entry.0 += 1;
            if is_win { day_entry.1 += 1; }
            day_entry.2 += trade.pnl_points;
        }

        // Print by hour
        println!("By Hour (ET):");
        let mut hours: Vec<_> = by_hour.iter().collect();
        hours.sort_by_key(|(h, _)| *h);
        for (hour, (total, wins, pnl)) in hours {
            let win_rate = if *total > 0 { *wins as f64 / *total as f64 * 100.0 } else { 0.0 };
            let hour_12 = if *hour == 0 { 12 } else if *hour > 12 { hour - 12 } else { *hour };
            let ampm = if *hour < 12 { "AM" } else { "PM" };
            println!("  {:2}:00 {}: {:3} trades, {:5.1}% WR, {:+7.2} pts",
                     hour_12, ampm, total, win_rate, pnl);
        }

        // Print by day
        println!("\nBy Day of Week:");
        let day_order = [
            chrono::Weekday::Mon,
            chrono::Weekday::Tue,
            chrono::Weekday::Wed,
            chrono::Weekday::Thu,
            chrono::Weekday::Fri,
        ];
        for day in &day_order {
            if let Some((total, wins, pnl)) = by_day.get(day) {
                let win_rate = if *total > 0 { *wins as f64 / *total as f64 * 100.0 } else { 0.0 };
                println!("  {:9}: {:3} trades, {:5.1}% WR, {:+7.2} pts",
                         format!("{:?}", day), total, win_rate, pnl);
            }
        }

        // Best/worst hours
        if let Some((best_hour, (_, _, best_pnl))) = by_hour.iter().max_by(|a, b| a.1.2.partial_cmp(&b.1.2).unwrap()) {
            if let Some((worst_hour, (_, _, worst_pnl))) = by_hour.iter().min_by(|a, b| a.1.2.partial_cmp(&b.1.2).unwrap()) {
                let best_12 = if *best_hour == 0 { 12 } else if *best_hour > 12 { best_hour - 12 } else { *best_hour };
                let best_ampm = if *best_hour < 12 { "AM" } else { "PM" };
                let worst_12 = if *worst_hour == 0 { 12 } else if *worst_hour > 12 { worst_hour - 12 } else { *worst_hour };
                let worst_ampm = if *worst_hour < 12 { "AM" } else { "PM" };
                println!("\n  Best hour:  {:2}:00 {} ({:+.2} pts)", best_12, best_ampm, best_pnl);
                println!("  Worst hour: {:2}:00 {} ({:+.2} pts)", worst_12, worst_ampm, worst_pnl);
            }
        }
    }

    println!("\n═══════════════════════════════════════════════════════════\n");
}
