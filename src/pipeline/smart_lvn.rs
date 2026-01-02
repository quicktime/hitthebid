//! Smart LVN Retest Strategy
//!
//! Implements the discretionary trader's actual process:
//! 1. Valid impulse = Balanced → Imbalanced market state transition
//! 2. First touch only (trapped traders)
//! 3. Delta confirmation AT the level (not just price touch)
//! 4. Max trades per day limit
//!
//! This captures the QUALITY filters that make the strategy profitable.

use crate::bars::Bar;
use crate::impulse::ImpulseDirection;
use crate::lvn::LvnLevel;
use crate::market_state::{detect_market_state, MarketState, MarketStateConfig, MarketStateResult};
use crate::precompute::DayData;
use tracing::debug;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use chrono_tz::America::New_York;
use std::collections::HashSet;

/// Configuration for smart LVN strategy
#[derive(Debug, Clone)]
pub struct SmartLvnConfig {
    /// Maximum trades per day
    pub max_trades_per_day: usize,
    /// Minimum delta at level to confirm entry (absolute value)
    pub min_delta_confirmation: i64,
    /// Minimum impulse size in points
    pub min_impulse_size: f64,
    /// Maximum bars for impulse (keep it "fast")
    pub max_impulse_bars: usize,
    /// Level tolerance for "at level" detection
    pub level_tolerance: f64,
    /// Trailing stop in points
    pub trailing_stop: f64,
    /// Take profit in points (0 = use trailing only)
    pub take_profit: f64,
    /// Stop buffer beyond LVN
    pub stop_buffer: f64,
    /// Trading start hour (ET)
    pub start_hour: u32,
    /// Trading end hour (ET)
    pub end_hour: u32,
}

impl Default for SmartLvnConfig {
    fn default() -> Self {
        Self {
            max_trades_per_day: 5,
            min_delta_confirmation: 30,  // Require real aggression
            min_impulse_size: 25.0,      // Decent size move
            max_impulse_bars: 120,       // 2 minutes max for "fast" impulse
            level_tolerance: 2.0,        // 2 points tolerance
            trailing_stop: 4.0,
            take_profit: 20.0,
            stop_buffer: 2.0,
            start_hour: 9,               // 9:30 AM ET start
            end_hour: 15,                // 3:00 PM ET end (avoid close)
        }
    }
}

/// Represents a valid impulse that created an LVN
#[derive(Debug, Clone)]
pub struct ValidImpulse {
    pub start_idx: usize,
    pub end_idx: usize,
    pub start_price: f64,
    pub end_price: f64,
    pub direction: ImpulseDir,
    pub lvn_price: f64,        // The key LVN from this impulse
    pub impulse_delta: i64,    // Total delta during impulse
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImpulseDir {
    Up,
    Down,
}

/// Trade result
#[derive(Debug, Clone)]
pub struct SmartTrade {
    pub entry_time: DateTime<Utc>,
    pub exit_time: DateTime<Utc>,
    pub entry_price: f64,
    pub exit_price: f64,
    pub direction: ImpulseDir,
    pub pnl_points: f64,
    pub lvn_price: f64,
    pub entry_delta: i64,      // Delta that confirmed entry
    pub exit_reason: ExitReason,
}

#[derive(Debug, Clone, Copy)]
pub enum ExitReason {
    TakeProfit,
    TrailingStop,
    EndOfDay,
}

/// Smart LVN backtester
pub struct SmartLvnBacktest {
    config: SmartLvnConfig,
    ms_config: MarketStateConfig,
}

impl SmartLvnBacktest {
    pub fn new(config: SmartLvnConfig) -> Self {
        Self {
            config,
            ms_config: MarketStateConfig::default(),
        }
    }

    /// Run backtest on cached data
    pub fn run(&self, days: &[DayData]) -> BacktestResult {
        let mut all_trades = Vec::new();
        let mut total_impulses = 0;
        let mut total_valid_impulses = 0;

        for (day_idx, day) in days.iter().enumerate() {
            if day.bars_1s.is_empty() {
                continue;
            }

            // Detect valid impulses for this day
            let impulses = self.detect_valid_impulses(&day.bars_1s);
            total_impulses += impulses.len();

            // Filter to only state-transition impulses
            let valid_impulses: Vec<_> = impulses.into_iter()
                .filter(|imp| self.is_state_transition_impulse(&day.bars_1s, imp))
                .collect();
            total_valid_impulses += valid_impulses.len();

            // Trade the valid impulses
            let day_trades = self.trade_day(&day.bars_1s, &valid_impulses);
            all_trades.extend(day_trades);
        }

        self.compute_results(all_trades, total_impulses, total_valid_impulses, days.len())
    }

    /// Detect potential impulse moves in bars
    fn detect_valid_impulses(&self, bars: &[Bar]) -> Vec<ValidImpulse> {
        let mut impulses = Vec::new();

        if bars.len() < self.config.max_impulse_bars {
            return impulses;
        }

        let mut i = 0;
        while i < bars.len() - self.config.max_impulse_bars {
            // Check if within trading hours
            let et_time = bars[i].timestamp.with_timezone(&New_York);
            let hour = et_time.hour();
            let minute = et_time.minute();

            let in_session = (hour > self.config.start_hour ||
                             (hour == self.config.start_hour && minute >= 30))
                            && hour < self.config.end_hour;

            if !in_session {
                i += 1;
                continue;
            }

            // Look for impulse starting here
            if let Some(impulse) = self.find_impulse_from(bars, i) {
                impulses.push(impulse.clone());
                i = impulse.end_idx + 1;  // Skip past this impulse
            } else {
                i += 1;
            }
        }

        impulses
    }

    /// Find an impulse move starting from given index
    fn find_impulse_from(&self, bars: &[Bar], start_idx: usize) -> Option<ValidImpulse> {
        let start_bar = &bars[start_idx];
        let start_price = start_bar.open;

        let mut high = start_bar.high;
        let mut low = start_bar.low;
        let mut total_delta: i64 = 0;

        for len in 1..=self.config.max_impulse_bars.min(bars.len() - start_idx) {
            let end_idx = start_idx + len - 1;
            let bar = &bars[end_idx];

            high = high.max(bar.high);
            low = low.min(bar.low);
            total_delta += bar.delta;

            let up_move = high - start_price;
            let down_move = start_price - low;

            // Check if we have a valid impulse
            if up_move >= self.config.min_impulse_size && up_move > down_move * 2.0 {
                // Up impulse - find the REAL LVN from volume profile
                // Fallback to 38.2% if no proper LVN found
                let lvn_price = find_best_lvn_in_impulse(bars, start_idx, end_idx, ImpulseDir::Up)
                    .unwrap_or(start_price + up_move * 0.382);

                return Some(ValidImpulse {
                    start_idx,
                    end_idx,
                    start_price,
                    end_price: high,
                    direction: ImpulseDir::Up,
                    lvn_price,
                    impulse_delta: total_delta,
                });
            } else if down_move >= self.config.min_impulse_size && down_move > up_move * 2.0 {
                // Down impulse - find the REAL LVN from volume profile
                let lvn_price = find_best_lvn_in_impulse(bars, start_idx, end_idx, ImpulseDir::Down)
                    .unwrap_or(start_price - down_move * 0.382);

                return Some(ValidImpulse {
                    start_idx,
                    end_idx,
                    start_price,
                    end_price: low,
                    direction: ImpulseDir::Down,
                    lvn_price,
                    impulse_delta: total_delta,
                });
            }
        }

        None
    }

    /// Check if impulse represents a Balanced → Imbalanced state transition
    fn is_state_transition_impulse(&self, bars: &[Bar], impulse: &ValidImpulse) -> bool {
        // Check market state BEFORE impulse
        let before_state = if impulse.start_idx >= self.ms_config.lookback_bars {
            detect_market_state(bars, impulse.start_idx - 1, &self.ms_config)
        } else {
            // Not enough history - check if impulse delta is strong enough by itself
            MarketStateResult {
                state: MarketState::Balanced,  // Assume balanced if we can't check
                fair_value: 0.0,
                atr: 0.0,
                rotation_count: 0,
                range_ratio: 0.0,
                cumulative_delta: 0,
                trend_direction: 0,
            }
        };

        // Check market state AFTER impulse
        let after_state = detect_market_state(bars, impulse.end_idx, &self.ms_config);

        // Valid impulse if:
        // 1. Classic transition: Balanced → Imbalanced, OR
        // 2. Strong impulse delta (shows conviction) regardless of prior state
        let is_classic_transition = before_state.state == MarketState::Balanced
            && after_state.state == MarketState::Imbalanced;

        // Strong delta during impulse indicates conviction
        let strong_delta = impulse.impulse_delta.abs() > 100;

        // Accept if either condition is met
        is_classic_transition || (after_state.state == MarketState::Imbalanced && strong_delta)
    }

    /// Trade a single day with given valid impulses
    fn trade_day(&self, bars: &[Bar], impulses: &[ValidImpulse]) -> Vec<SmartTrade> {
        let mut trades = Vec::new();
        let mut used_lvns: HashSet<i64> = HashSet::new();  // Track first-touch
        let mut daily_trades = 0;
        let mut in_position_until: usize = 0;  // Don't enter new trades until this bar index

        for impulse in impulses {
            if daily_trades >= self.config.max_trades_per_day {
                break;
            }

            // Check if this LVN has been used (first touch only)
            let lvn_bucket = (impulse.lvn_price * 4.0) as i64;  // 0.25 point granularity
            if used_lvns.contains(&lvn_bucket) {
                continue;  // Already traded this level
            }

            // Don't search before previous trade exits (one trade at a time)
            let search_start = (impulse.end_idx + 1).max(in_position_until);

            // Look for retest after impulse
            if let Some(trade) = self.find_retest_trade(bars, impulse, search_start) {
                // Find the exit bar index
                let exit_idx = bars.iter()
                    .position(|b| b.timestamp >= trade.exit_time)
                    .unwrap_or(bars.len());
                in_position_until = exit_idx + 1;

                used_lvns.insert(lvn_bucket);
                trades.push(trade);
                daily_trades += 1;
            }
        }

        trades
    }

    /// Find a valid retest trade
    fn find_retest_trade(&self, bars: &[Bar], impulse: &ValidImpulse, search_start: usize) -> Option<SmartTrade> {
        let max_search = (search_start + 3600).min(bars.len());  // Look up to 1 hour

        for i in search_start..max_search {
            let bar = &bars[i];

            // Check if within trading hours
            let et_time = bar.timestamp.with_timezone(&New_York);
            let hour = et_time.hour();
            if hour >= self.config.end_hour {
                return None;  // Past trading hours
            }

            // Check if price is at LVN level
            let at_level = (bar.low <= impulse.lvn_price + self.config.level_tolerance) &&
                          (bar.high >= impulse.lvn_price - self.config.level_tolerance);

            if !at_level {
                continue;
            }

            // KEY: Check for delta confirmation at level
            // For up impulse retest, we want BUYING aggression (positive delta)
            // For down impulse retest, we want SELLING aggression (negative delta)
            let delta_confirms = match impulse.direction {
                ImpulseDir::Up => bar.delta >= self.config.min_delta_confirmation as i64,
                ImpulseDir::Down => bar.delta <= -(self.config.min_delta_confirmation as i64),
            };

            if !delta_confirms {
                continue;  // No aggression = no trade
            }

            // We have a valid entry signal!
            let entry_price = bar.close;
            let entry_time = bar.timestamp;
            let entry_delta = bar.delta;

            // Simulate trade exit
            return self.simulate_exit(bars, i, entry_price, impulse.direction, impulse.lvn_price)
                .map(|(exit_price, exit_time, exit_reason)| {
                    let pnl = match impulse.direction {
                        ImpulseDir::Up => exit_price - entry_price,
                        ImpulseDir::Down => entry_price - exit_price,
                    };

                    SmartTrade {
                        entry_time,
                        exit_time,
                        entry_price,
                        exit_price,
                        direction: impulse.direction,
                        pnl_points: pnl,
                        lvn_price: impulse.lvn_price,
                        entry_delta,
                        exit_reason,
                    }
                });
        }

        None
    }

    /// Simulate trade exit with trailing stop
    fn simulate_exit(
        &self,
        bars: &[Bar],
        entry_idx: usize,
        entry_price: f64,
        direction: ImpulseDir,
        lvn_price: f64,
    ) -> Option<(f64, DateTime<Utc>, ExitReason)> {
        let stop_price = match direction {
            ImpulseDir::Up => lvn_price - self.config.stop_buffer,
            ImpulseDir::Down => lvn_price + self.config.stop_buffer,
        };

        let mut best_price = entry_price;
        let mut trailing_stop = stop_price;

        for i in (entry_idx + 1)..bars.len() {
            let bar = &bars[i];

            // Check end of day
            let et_time = bar.timestamp.with_timezone(&New_York);
            if et_time.hour() >= 16 {
                return Some((bar.close, bar.timestamp, ExitReason::EndOfDay));
            }

            match direction {
                ImpulseDir::Up => {
                    // Update best price and trailing stop
                    if bar.high > best_price {
                        best_price = bar.high;
                        trailing_stop = best_price - self.config.trailing_stop;
                    }

                    // Check take profit
                    if self.config.take_profit > 0.0 && bar.high >= entry_price + self.config.take_profit {
                        return Some((entry_price + self.config.take_profit, bar.timestamp, ExitReason::TakeProfit));
                    }

                    // Check stop
                    if bar.low <= trailing_stop {
                        return Some((trailing_stop, bar.timestamp, ExitReason::TrailingStop));
                    }
                }
                ImpulseDir::Down => {
                    if bar.low < best_price {
                        best_price = bar.low;
                        trailing_stop = best_price + self.config.trailing_stop;
                    }

                    if self.config.take_profit > 0.0 && bar.low <= entry_price - self.config.take_profit {
                        return Some((entry_price - self.config.take_profit, bar.timestamp, ExitReason::TakeProfit));
                    }

                    if bar.high >= trailing_stop {
                        return Some((trailing_stop, bar.timestamp, ExitReason::TrailingStop));
                    }
                }
            }
        }

        // End of data
        let last_bar = bars.last()?;
        Some((last_bar.close, last_bar.timestamp, ExitReason::EndOfDay))
    }

    /// Compute backtest results
    fn compute_results(
        &self,
        trades: Vec<SmartTrade>,
        total_impulses: usize,
        valid_impulses: usize,
        total_days: usize,
    ) -> BacktestResult {
        let total_trades = trades.len();
        let wins: Vec<_> = trades.iter().filter(|t| t.pnl_points > 0.0).collect();
        let losses: Vec<_> = trades.iter().filter(|t| t.pnl_points < 0.0).collect();

        let win_count = wins.len();
        let loss_count = losses.len();
        let win_rate = if total_trades > 0 { win_count as f64 / total_trades as f64 * 100.0 } else { 0.0 };

        let gross_profit: f64 = wins.iter().map(|t| t.pnl_points).sum();
        let gross_loss: f64 = losses.iter().map(|t| t.pnl_points.abs()).sum();
        let net_pnl: f64 = trades.iter().map(|t| t.pnl_points).sum();

        let profit_factor = if gross_loss > 0.0 { gross_profit / gross_loss } else { f64::INFINITY };

        let avg_win = if win_count > 0 { gross_profit / win_count as f64 } else { 0.0 };
        let avg_loss = if loss_count > 0 { gross_loss / loss_count as f64 } else { 0.0 };

        BacktestResult {
            total_days,
            total_impulses,
            valid_impulses,
            total_trades,
            wins: win_count,
            losses: loss_count,
            win_rate,
            profit_factor,
            avg_win,
            avg_loss,
            net_pnl,
            gross_profit,
            gross_loss,
            trades,
        }
    }
}

/// Backtest results
#[derive(Debug)]
pub struct BacktestResult {
    pub total_days: usize,
    pub total_impulses: usize,
    pub valid_impulses: usize,
    pub total_trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub net_pnl: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub trades: Vec<SmartTrade>,
}

/// Exit sweep result for one configuration
#[derive(Debug, Clone)]
pub struct ExitSweepResult {
    pub trailing_stop: f64,
    pub take_profit: f64,
    pub total_trades: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub net_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub tp_count: usize,
    pub ts_count: usize,
    pub tp_pnl: f64,
    pub ts_pnl: f64,
}

/// Run exit parameter sweep with fixed entry parameters
pub fn run_exit_sweep(
    days: &[DayData],
    min_delta: i64,
    min_impulse_size: f64,
    trailing_stops: &[f64],
    take_profits: &[f64],
) -> Vec<ExitSweepResult> {
    let mut results = Vec::new();

    // Fixed entry parameters (best performing from entry optimization)
    let base_config = SmartLvnConfig {
        max_trades_per_day: 5,
        min_delta_confirmation: min_delta,
        min_impulse_size,
        max_impulse_bars: 120,
        level_tolerance: 2.0,
        trailing_stop: 4.0,  // Will be overridden
        take_profit: 20.0,   // Will be overridden
        stop_buffer: 2.0,
        start_hour: 9,
        end_hour: 15,
    };

    for &trail in trailing_stops {
        for &tp in take_profits {
            let mut config = base_config.clone();
            config.trailing_stop = trail;
            config.take_profit = tp;

            let backtest = SmartLvnBacktest::new(config);
            let result = backtest.run(days);

            // Count exit types
            let tp_exits: Vec<_> = result.trades.iter()
                .filter(|t| matches!(t.exit_reason, ExitReason::TakeProfit))
                .collect();
            let ts_exits: Vec<_> = result.trades.iter()
                .filter(|t| matches!(t.exit_reason, ExitReason::TrailingStop))
                .collect();

            let tp_pnl: f64 = tp_exits.iter().map(|t| t.pnl_points).sum();
            let ts_pnl: f64 = ts_exits.iter().map(|t| t.pnl_points).sum();

            results.push(ExitSweepResult {
                trailing_stop: trail,
                take_profit: tp,
                total_trades: result.total_trades,
                wins: result.wins,
                win_rate: result.win_rate,
                profit_factor: result.profit_factor,
                net_pnl: result.net_pnl,
                avg_win: result.avg_win,
                avg_loss: result.avg_loss,
                tp_count: tp_exits.len(),
                ts_count: ts_exits.len(),
                tp_pnl,
                ts_pnl,
            });
        }
    }

    // Sort by profit factor (descending), then by net P&L
    results.sort_by(|a, b| {
        b.profit_factor.partial_cmp(&a.profit_factor)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.net_pnl.partial_cmp(&a.net_pnl).unwrap_or(std::cmp::Ordering::Equal))
    });

    results
}

/// Time window for trading hours filter
#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    pub start_hour: u32,
    pub end_hour: u32,
    pub name: &'static str,
}

impl TimeWindow {
    pub const ALL_DAY: TimeWindow = TimeWindow { start_hour: 9, end_hour: 16, name: "all_day" };
    pub const MORNING: TimeWindow = TimeWindow { start_hour: 9, end_hour: 12, name: "morning" };
    pub const MIDDAY: TimeWindow = TimeWindow { start_hour: 10, end_hour: 14, name: "midday" };
    pub const AFTERNOON: TimeWindow = TimeWindow { start_hour: 12, end_hour: 16, name: "afternoon" };
    pub const OPEN_HOUR: TimeWindow = TimeWindow { start_hour: 9, end_hour: 11, name: "open_hour" };
}

/// Result from multi-dimensional sweep
#[derive(Debug, Clone)]
pub struct MultiSweepResult {
    pub min_delta: i64,
    pub trailing_stop: f64,
    pub min_impulse_size: f64,
    pub time_window: &'static str,
    pub start_hour: u32,
    pub end_hour: u32,
    pub total_trades: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub net_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub trades_per_day: f64,
}

/// Run comprehensive multi-dimensional parameter sweep
/// Tests ALL combinations of:
/// - min_delta
/// - trailing_stop
/// - min_impulse_size
/// - time_window
pub fn run_multi_sweep(
    days: &[DayData],
    deltas: &[i64],
    trailing_stops: &[f64],
    impulse_sizes: &[f64],
    time_windows: &[TimeWindow],
) -> Vec<MultiSweepResult> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use rayon::prelude::*;

    let total_combinations = deltas.len() * trailing_stops.len() * impulse_sizes.len() * time_windows.len();
    println!("Running {} parameter combinations...", total_combinations);

    let progress = AtomicUsize::new(0);

    // Build all parameter combinations
    let mut combinations = Vec::new();
    for &delta in deltas {
        for &trail in trailing_stops {
            for &impulse_size in impulse_sizes {
                for window in time_windows {
                    combinations.push((delta, trail, impulse_size, *window));
                }
            }
        }
    }

    // Run in parallel
    let results: Vec<MultiSweepResult> = combinations
        .par_iter()
        .map(|(delta, trail, impulse_size, window)| {
            let config = SmartLvnConfig {
                max_trades_per_day: 5,
                min_delta_confirmation: *delta,
                min_impulse_size: *impulse_size,
                max_impulse_bars: 120,
                level_tolerance: 2.0,
                trailing_stop: *trail,
                take_profit: 0.0,  // Trailing only
                stop_buffer: 2.0,
                start_hour: window.start_hour,
                end_hour: window.end_hour,
            };

            let backtest = SmartLvnBacktest::new(config);
            let result = backtest.run(days);

            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 50 == 0 || done == total_combinations {
                eprint!("\rProgress: {}/{} ({:.0}%)", done, total_combinations, 100.0 * done as f64 / total_combinations as f64);
            }

            MultiSweepResult {
                min_delta: *delta,
                trailing_stop: *trail,
                min_impulse_size: *impulse_size,
                time_window: window.name,
                start_hour: window.start_hour,
                end_hour: window.end_hour,
                total_trades: result.total_trades,
                wins: result.wins,
                win_rate: result.win_rate,
                profit_factor: result.profit_factor,
                net_pnl: result.net_pnl,
                avg_win: result.avg_win,
                avg_loss: result.avg_loss,
                trades_per_day: result.total_trades as f64 / days.len().max(1) as f64,
            }
        })
        .collect();

    eprintln!();  // Newline after progress

    // Sort by profit factor (with minimum trade threshold)
    let mut results = results;
    results.sort_by(|a, b| {
        // Penalize configs with too few trades (less than 0.3/day)
        let a_valid = a.trades_per_day >= 0.3;
        let b_valid = b.trades_per_day >= 0.3;

        match (a_valid, b_valid) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.profit_factor.partial_cmp(&a.profit_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.net_pnl.partial_cmp(&a.net_pnl).unwrap_or(std::cmp::Ordering::Equal))
        }
    });

    results
}

/// Print multi-sweep results
pub fn print_multi_sweep_results(results: &[MultiSweepResult], top_n: usize) {
    println!("\n═══════════════════════════════════════════════════════════════════════════════════════════════════");
    println!("                              MULTI-DIMENSIONAL PARAMETER SWEEP RESULTS                             ");
    println!("═══════════════════════════════════════════════════════════════════════════════════════════════════\n");

    println!("{:>6} {:>5} {:>7} {:>12} {:>6} {:>5} {:>6} {:>5} {:>8} {:>7} {:>7} {:>6}",
        "Delta", "Trail", "Impulse", "TimeWindow", "Trades", "Wins", "Win%", "PF", "Net P&L", "AvgWin", "AvgLos", "T/Day");
    println!("{}", "-".repeat(100));

    for r in results.iter().take(top_n) {
        let pf_str = if r.profit_factor > 10.0 { ">10".to_string() } else { format!("{:.2}", r.profit_factor) };

        println!("{:>6} {:>5.1} {:>7.0} {:>12} {:>6} {:>5} {:>5.1}% {:>5} {:>+8.1} {:>7.2} {:>7.2} {:>6.2}",
            r.min_delta,
            r.trailing_stop,
            r.min_impulse_size,
            r.time_window,
            r.total_trades,
            r.wins,
            r.win_rate,
            pf_str,
            r.net_pnl,
            r.avg_win,
            r.avg_loss,
            r.trades_per_day);
    }

    println!("\n═══════════════════════════════════════════════════════════════════════════════════════════════════\n");

    // Show best by different metrics
    if let Some(best_pf) = results.iter().filter(|r| r.trades_per_day >= 0.3).max_by(|a, b|
        a.profit_factor.partial_cmp(&b.profit_factor).unwrap_or(std::cmp::Ordering::Equal)
    ) {
        println!("BEST PROFIT FACTOR (min 0.3 trades/day):");
        println!("  Delta={}, Trail={}, Impulse={}, Time={} → PF={:.2}, Net={:+.1}, {:.2} trades/day",
            best_pf.min_delta, best_pf.trailing_stop, best_pf.min_impulse_size,
            best_pf.time_window, best_pf.profit_factor, best_pf.net_pnl, best_pf.trades_per_day);
    }

    if let Some(best_pnl) = results.iter().filter(|r| r.trades_per_day >= 0.3).max_by(|a, b|
        a.net_pnl.partial_cmp(&b.net_pnl).unwrap_or(std::cmp::Ordering::Equal)
    ) {
        println!("\nBEST NET P&L (min 0.3 trades/day):");
        println!("  Delta={}, Trail={}, Impulse={}, Time={} → Net={:+.1}, PF={:.2}, {:.2} trades/day",
            best_pnl.min_delta, best_pnl.trailing_stop, best_pnl.min_impulse_size,
            best_pnl.time_window, best_pnl.net_pnl, best_pnl.profit_factor, best_pnl.trades_per_day);
    }

    // Find best PF with at least 1 trade/day
    if let Some(best_active) = results.iter().filter(|r| r.trades_per_day >= 1.0).max_by(|a, b|
        a.profit_factor.partial_cmp(&b.profit_factor).unwrap_or(std::cmp::Ordering::Equal)
    ) {
        println!("\nBEST PF WITH 1+ TRADE/DAY:");
        println!("  Delta={}, Trail={}, Impulse={}, Time={} → PF={:.2}, Net={:+.1}, {:.2} trades/day",
            best_active.min_delta, best_active.trailing_stop, best_active.min_impulse_size,
            best_active.time_window, best_active.profit_factor, best_active.net_pnl, best_active.trades_per_day);
    }

    println!();
}

/// Compute proper LVN from bars during impulse
/// Returns the single best LVN - the thinnest spot with trapped traders on both sides
pub fn find_best_lvn_in_impulse(
    bars: &[Bar],
    start_idx: usize,
    end_idx: usize,
    _direction: ImpulseDir,  // Reserved for future use
) -> Option<f64> {
    if end_idx <= start_idx || end_idx >= bars.len() {
        return None;
    }

    // Build volume profile with larger buckets (2 points = 8 ticks)
    const BUCKET_SIZE: f64 = 2.0;

    let mut volume_at_price: std::collections::HashMap<i64, u64> = std::collections::HashMap::new();
    let mut price_min = f64::MAX;
    let mut price_max = f64::MIN;

    for i in start_idx..=end_idx {
        let bar = &bars[i];
        let bucket = (bar.close / BUCKET_SIZE).round() as i64;
        *volume_at_price.entry(bucket).or_insert(0) += bar.volume as u64;
        price_min = price_min.min(bar.low);
        price_max = price_max.max(bar.high);
    }

    if volume_at_price.len() < 5 {
        return None; // Need enough price levels to find a meaningful LVN
    }

    // Calculate impulse range
    let impulse_range = price_max - price_min;
    if impulse_range < 10.0 {
        return None; // Too small to have meaningful LVN
    }

    // Find the thinnest bucket IN THE MIDDLE 60% of the impulse (not at extremes)
    let middle_low = price_min + impulse_range * 0.2;
    let middle_high = price_max - impulse_range * 0.2;

    let avg_volume: f64 = volume_at_price.values().sum::<u64>() as f64 / volume_at_price.len() as f64;

    let mut best_lvn: Option<(f64, f64)> = None; // (price, volume_ratio)

    for (&bucket, &volume) in &volume_at_price {
        let price = bucket as f64 * BUCKET_SIZE;

        // Must be in middle portion
        if price < middle_low || price > middle_high {
            continue;
        }

        let volume_ratio = volume as f64 / avg_volume;

        // Must be significantly thin (< 30% of average)
        if volume_ratio > 0.30 {
            continue;
        }

        // Check that there's volume on BOTH sides (trapped traders)
        let has_volume_above = volume_at_price.iter()
            .any(|(&b, &v)| b as f64 * BUCKET_SIZE > price + BUCKET_SIZE && v as f64 > avg_volume * 0.5);
        let has_volume_below = volume_at_price.iter()
            .any(|(&b, &v)| (b as f64 * BUCKET_SIZE) < price - BUCKET_SIZE && v as f64 > avg_volume * 0.5);

        if !has_volume_above || !has_volume_below {
            continue;
        }

        // Track the thinnest one
        if best_lvn.is_none() || volume_ratio < best_lvn.unwrap().1 {
            best_lvn = Some((price, volume_ratio));
        }
    }

    best_lvn.map(|(price, _)| price)
}

/// Real LVN backtester - uses actual volume profile LVNs from precomputed data
pub struct RealLvnBacktest {
    config: SmartLvnConfig,
}

impl RealLvnBacktest {
    pub fn new(config: SmartLvnConfig) -> Self {
        Self { config }
    }

    /// Run backtest using REAL LVN levels from precomputed data
    pub fn run(&self, days: &[DayData]) -> BacktestResult {
        let mut all_trades = Vec::new();
        let mut total_lvns = 0;

        for day in days {
            if day.bars_1s.is_empty() || day.lvn_levels.is_empty() {
                continue;
            }

            total_lvns += day.lvn_levels.len();

            // Trade real LVNs for this day
            let day_trades = self.trade_day_real_lvns(&day.bars_1s, &day.lvn_levels);
            all_trades.extend(day_trades);
        }

        self.compute_results(all_trades, total_lvns, days.len())
    }

    /// Trade a single day using real LVN levels
    fn trade_day_real_lvns(&self, bars: &[Bar], lvns: &[LvnLevel]) -> Vec<SmartTrade> {
        let mut trades = Vec::new();
        let mut used_lvns: HashSet<i64> = HashSet::new();
        let mut daily_trades = 0;

        for lvn in lvns {
            if daily_trades >= self.config.max_trades_per_day {
                break;
            }

            // Skip low quality LVNs (volume ratio too high = not thin enough)
            if lvn.volume_ratio > 0.15 {
                continue;
            }

            // Check if this LVN has been used (first touch only)
            let lvn_bucket = (lvn.price * 4.0) as i64;
            if used_lvns.contains(&lvn_bucket) {
                continue;
            }

            // Find bars after LVN was created (impulse ended)
            let search_start = bars.iter()
                .position(|b| b.timestamp > lvn.impulse_end_time)
                .unwrap_or(0);

            if search_start == 0 {
                continue;
            }

            // Convert impulse direction
            let direction = match lvn.impulse_direction {
                ImpulseDirection::Up => ImpulseDir::Up,
                ImpulseDirection::Down => ImpulseDir::Down,
            };

            // Look for retest trade
            if let Some(trade) = self.find_real_lvn_retest(
                bars,
                lvn.price,
                direction,
                search_start,
            ) {
                used_lvns.insert(lvn_bucket);
                trades.push(trade);
                daily_trades += 1;
            }
        }

        trades
    }

    /// Find a valid retest trade at real LVN level
    fn find_real_lvn_retest(
        &self,
        bars: &[Bar],
        lvn_price: f64,
        direction: ImpulseDir,
        search_start: usize,
    ) -> Option<SmartTrade> {
        let max_search = (search_start + 7200).min(bars.len()); // 2 hours max

        for i in search_start..max_search {
            let bar = &bars[i];

            // Check if within trading hours
            let et_time = bar.timestamp.with_timezone(&New_York);
            let hour = et_time.hour();
            let minute = et_time.minute();

            let in_session = (hour > self.config.start_hour ||
                             (hour == self.config.start_hour && minute >= 30))
                            && hour < self.config.end_hour;

            if !in_session {
                continue;
            }

            // Check if price is at LVN level
            let at_level = (bar.low <= lvn_price + self.config.level_tolerance) &&
                          (bar.high >= lvn_price - self.config.level_tolerance);

            if !at_level {
                continue;
            }

            // KEY: Check for delta confirmation at level
            let delta_confirms = match direction {
                ImpulseDir::Up => bar.delta >= self.config.min_delta_confirmation as i64,
                ImpulseDir::Down => bar.delta <= -(self.config.min_delta_confirmation as i64),
            };

            if !delta_confirms {
                continue;
            }

            // Valid entry signal
            let entry_price = bar.close;
            let entry_time = bar.timestamp;
            let entry_delta = bar.delta;

            // Simulate exit
            return self.simulate_exit_real(bars, i, entry_price, direction, lvn_price)
                .map(|(exit_price, exit_time, exit_reason)| {
                    let pnl = match direction {
                        ImpulseDir::Up => exit_price - entry_price,
                        ImpulseDir::Down => entry_price - exit_price,
                    };

                    SmartTrade {
                        entry_time,
                        exit_time,
                        entry_price,
                        exit_price,
                        direction,
                        pnl_points: pnl,
                        lvn_price,
                        entry_delta,
                        exit_reason,
                    }
                });
        }

        None
    }

    /// Simulate trade exit
    fn simulate_exit_real(
        &self,
        bars: &[Bar],
        entry_idx: usize,
        entry_price: f64,
        direction: ImpulseDir,
        lvn_price: f64,
    ) -> Option<(f64, DateTime<Utc>, ExitReason)> {
        let stop_price = match direction {
            ImpulseDir::Up => lvn_price - self.config.stop_buffer,
            ImpulseDir::Down => lvn_price + self.config.stop_buffer,
        };

        let mut best_price = entry_price;
        let mut trailing_stop = stop_price;

        for i in (entry_idx + 1)..bars.len() {
            let bar = &bars[i];

            let et_time = bar.timestamp.with_timezone(&New_York);
            if et_time.hour() >= 16 {
                return Some((bar.close, bar.timestamp, ExitReason::EndOfDay));
            }

            match direction {
                ImpulseDir::Up => {
                    if bar.high > best_price {
                        best_price = bar.high;
                        trailing_stop = best_price - self.config.trailing_stop;
                    }

                    if self.config.take_profit > 0.0 && bar.high >= entry_price + self.config.take_profit {
                        return Some((entry_price + self.config.take_profit, bar.timestamp, ExitReason::TakeProfit));
                    }

                    if bar.low <= trailing_stop {
                        return Some((trailing_stop, bar.timestamp, ExitReason::TrailingStop));
                    }
                }
                ImpulseDir::Down => {
                    if bar.low < best_price {
                        best_price = bar.low;
                        trailing_stop = best_price + self.config.trailing_stop;
                    }

                    if self.config.take_profit > 0.0 && bar.low <= entry_price - self.config.take_profit {
                        return Some((entry_price - self.config.take_profit, bar.timestamp, ExitReason::TakeProfit));
                    }

                    if bar.high >= trailing_stop {
                        return Some((trailing_stop, bar.timestamp, ExitReason::TrailingStop));
                    }
                }
            }
        }

        let last_bar = bars.last()?;
        Some((last_bar.close, last_bar.timestamp, ExitReason::EndOfDay))
    }

    fn compute_results(
        &self,
        trades: Vec<SmartTrade>,
        total_lvns: usize,
        total_days: usize,
    ) -> BacktestResult {
        let total_trades = trades.len();
        let wins: Vec<_> = trades.iter().filter(|t| t.pnl_points > 0.0).collect();
        let losses: Vec<_> = trades.iter().filter(|t| t.pnl_points < 0.0).collect();

        let win_count = wins.len();
        let loss_count = losses.len();
        let win_rate = if total_trades > 0 { win_count as f64 / total_trades as f64 * 100.0 } else { 0.0 };

        let gross_profit: f64 = wins.iter().map(|t| t.pnl_points).sum();
        let gross_loss: f64 = losses.iter().map(|t| t.pnl_points.abs()).sum();
        let net_pnl: f64 = trades.iter().map(|t| t.pnl_points).sum();

        let profit_factor = if gross_loss > 0.0 { gross_profit / gross_loss } else { f64::INFINITY };

        let avg_win = if win_count > 0 { gross_profit / win_count as f64 } else { 0.0 };
        let avg_loss = if loss_count > 0 { gross_loss / loss_count as f64 } else { 0.0 };

        BacktestResult {
            total_days,
            total_impulses: 0, // Not applicable for real LVN
            valid_impulses: total_lvns,
            total_trades,
            wins: win_count,
            losses: loss_count,
            win_rate,
            profit_factor,
            avg_win,
            avg_loss,
            net_pnl,
            gross_profit,
            gross_loss,
            trades,
        }
    }
}

/// Analyze delta distribution at LVN retests to calibrate threshold
pub fn analyze_delta_distribution(days: &[DayData], min_impulse_size: f64) {
    println!("\n═══════════════════════════════════════════════════════════════════════════════");
    println!("                     DELTA DISTRIBUTION ANALYSIS                                ");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    // Collect all potential entry points with their delta values and outcomes
    // Use very low delta threshold (10) to capture all potential entries
    let config = SmartLvnConfig {
        max_trades_per_day: 100, // No limit for analysis
        min_delta_confirmation: 10, // Very low to capture all
        min_impulse_size,
        max_impulse_bars: 120,
        level_tolerance: 2.0,
        trailing_stop: 4.0,
        take_profit: 0.0,
        stop_buffer: 2.0,
        start_hour: 9,
        end_hour: 15,
    };

    let backtest = SmartLvnBacktest::new(config);
    let result = backtest.run(days);

    if result.trades.is_empty() {
        println!("No trades found for analysis.");
        return;
    }

    // Analyze delta distribution
    let mut deltas: Vec<(i64, f64)> = result.trades.iter()
        .map(|t| (t.entry_delta.abs(), t.pnl_points))
        .collect();

    deltas.sort_by_key(|(d, _)| *d);

    // Bucket analysis
    let buckets = [
        (0, 50, "0-50"),
        (50, 100, "50-100"),
        (100, 150, "100-150"),
        (150, 200, "150-200"),
        (200, 300, "200-300"),
        (300, 500, "300-500"),
        (500, 10000, "500+"),
    ];

    println!("{:>10} {:>8} {:>8} {:>8} {:>10} {:>10}",
        "Delta", "Trades", "Wins", "Win%", "Avg P&L", "Total P&L");
    println!("{}", "-".repeat(65));

    for (min, max, label) in &buckets {
        let bucket_trades: Vec<_> = deltas.iter()
            .filter(|(d, _)| *d >= *min && *d < *max)
            .collect();

        if bucket_trades.is_empty() {
            continue;
        }

        let count = bucket_trades.len();
        let wins = bucket_trades.iter().filter(|(_, p)| *p > 0.0).count();
        let win_rate = wins as f64 / count as f64 * 100.0;
        let total_pnl: f64 = bucket_trades.iter().map(|(_, p)| p).sum();
        let avg_pnl = total_pnl / count as f64;

        println!("{:>10} {:>8} {:>8} {:>7.1}% {:>+10.2} {:>+10.1}",
            label, count, wins, win_rate, avg_pnl, total_pnl);
    }

    // Find optimal threshold
    println!("\n─── CUMULATIVE ANALYSIS (trades WITH delta >= threshold) ───\n");

    let thresholds = [50, 75, 100, 125, 150, 175, 200, 250, 300];

    println!("{:>10} {:>8} {:>8} {:>8} {:>10} {:>8}",
        "Min Delta", "Trades", "Wins", "Win%", "Net P&L", "PF");
    println!("{}", "-".repeat(60));

    for threshold in &thresholds {
        let filtered: Vec<_> = deltas.iter()
            .filter(|(d, _)| *d >= *threshold)
            .collect();

        if filtered.is_empty() {
            continue;
        }

        let count = filtered.len();
        let wins = filtered.iter().filter(|(_, p)| *p > 0.0).count();
        let win_rate = wins as f64 / count as f64 * 100.0;
        let gross_profit: f64 = filtered.iter().filter(|(_, p)| *p > 0.0).map(|(_, p)| p).sum();
        let gross_loss: f64 = filtered.iter().filter(|(_, p)| *p < 0.0).map(|(_, p)| p.abs()).sum();
        let net_pnl: f64 = filtered.iter().map(|(_, p)| p).sum();
        let pf = if gross_loss > 0.0 { gross_profit / gross_loss } else { f64::INFINITY };

        let pf_str = if pf > 10.0 { ">10".to_string() } else { format!("{:.2}", pf) };

        println!("{:>10} {:>8} {:>8} {:>7.1}% {:>+10.1} {:>8}",
            threshold, count, wins, win_rate, net_pnl, pf_str);
    }

    // Show percentiles
    println!("\n─── DELTA PERCENTILES ───\n");
    let n = deltas.len();
    let p10 = deltas[n / 10].0;
    let p25 = deltas[n / 4].0;
    let p50 = deltas[n / 2].0;
    let p75 = deltas[3 * n / 4].0;
    let p90 = deltas[9 * n / 10].0;

    println!("10th percentile: {} delta", p10);
    println!("25th percentile: {} delta", p25);
    println!("50th percentile: {} delta (median)", p50);
    println!("75th percentile: {} delta", p75);
    println!("90th percentile: {} delta", p90);

    println!("\n═══════════════════════════════════════════════════════════════════════════════\n");
}

/// Print exit sweep results
pub fn print_exit_sweep_results(results: &[ExitSweepResult], min_delta: i64, min_impulse_size: f64) {
    println!("\n═══════════════════════════════════════════════════════════════════════════════");
    println!("                     EXIT STRATEGY SWEEP RESULTS                               ");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    println!("Fixed entry parameters: delta={}, impulse_size={}", min_delta, min_impulse_size);
    println!();

    println!("{:>6} {:>6} {:>6} {:>5} {:>6} {:>5} {:>8} {:>7} {:>7} {:>6} {:>6} {:>8} {:>8}",
        "Trail", "TP", "Trades", "Wins", "Win%", "PF", "Net P&L", "AvgWin", "AvgLos", "TP#", "TS#", "TP P&L", "TS P&L");
    println!("{}", "-".repeat(95));

    for r in results.iter().take(20) {
        let pf_str = if r.profit_factor > 10.0 { ">10".to_string() } else { format!("{:.2}", r.profit_factor) };

        println!("{:>6.1} {:>6.1} {:>6} {:>5} {:>5.1}% {:>5} {:>+8.1} {:>7.2} {:>7.2} {:>6} {:>6} {:>+8.1} {:>+8.1}",
            r.trailing_stop,
            r.take_profit,
            r.total_trades,
            r.wins,
            r.win_rate,
            pf_str,
            r.net_pnl,
            r.avg_win,
            r.avg_loss,
            r.tp_count,
            r.ts_count,
            r.tp_pnl,
            r.ts_pnl);
    }

    println!("\n═══════════════════════════════════════════════════════════════════════════════\n");
}

impl std::fmt::Display for BacktestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n═══════════════════════════════════════════════════════════")?;
        writeln!(f, "           SMART LVN BACKTEST RESULTS                       ")?;
        writeln!(f, "═══════════════════════════════════════════════════════════\n")?;

        writeln!(f, "Days Analyzed:     {}", self.total_days)?;
        writeln!(f, "Total Impulses:    {}", self.total_impulses)?;
        writeln!(f, "Valid Impulses:    {} (Balanced→Imbalanced)", self.valid_impulses)?;
        writeln!(f, "Trades/Day:        {:.1}", self.total_trades as f64 / self.total_days.max(1) as f64)?;
        writeln!(f)?;
        writeln!(f, "Total Trades:      {}", self.total_trades)?;
        writeln!(f, "Wins:              {} ({:.1}%)", self.wins, self.win_rate)?;
        writeln!(f, "Losses:            {}", self.losses)?;
        writeln!(f)?;
        writeln!(f, "Profit Factor:     {:.2}", self.profit_factor)?;
        writeln!(f, "Avg Win:           {:.2} pts", self.avg_win)?;
        writeln!(f, "Avg Loss:          {:.2} pts", self.avg_loss)?;
        writeln!(f)?;
        writeln!(f, "Gross Profit:      {:+.2} pts", self.gross_profit)?;
        writeln!(f, "Gross Loss:        -{:.2} pts", self.gross_loss)?;
        writeln!(f, "Net P&L:           {:+.2} pts", self.net_pnl)?;

        // Exit breakdown
        let tp_exits: Vec<_> = self.trades.iter().filter(|t| matches!(t.exit_reason, ExitReason::TakeProfit)).collect();
        let ts_exits: Vec<_> = self.trades.iter().filter(|t| matches!(t.exit_reason, ExitReason::TrailingStop)).collect();
        let eod_exits: Vec<_> = self.trades.iter().filter(|t| matches!(t.exit_reason, ExitReason::EndOfDay)).collect();

        let tp_pnl: f64 = tp_exits.iter().map(|t| t.pnl_points).sum();
        let ts_pnl: f64 = ts_exits.iter().map(|t| t.pnl_points).sum();
        let eod_pnl: f64 = eod_exits.iter().map(|t| t.pnl_points).sum();

        writeln!(f)?;
        writeln!(f, "─── EXIT BREAKDOWN ───")?;
        writeln!(f, "Take Profit:       {} trades, {:+.1} pts avg, {:+.1} total",
            tp_exits.len(),
            if tp_exits.is_empty() { 0.0 } else { tp_pnl / tp_exits.len() as f64 },
            tp_pnl)?;
        writeln!(f, "Trailing Stop:     {} trades, {:+.1} pts avg, {:+.1} total",
            ts_exits.len(),
            if ts_exits.is_empty() { 0.0 } else { ts_pnl / ts_exits.len() as f64 },
            ts_pnl)?;
        writeln!(f, "End of Day:        {} trades, {:+.1} pts avg, {:+.1} total",
            eod_exits.len(),
            if eod_exits.is_empty() { 0.0 } else { eod_pnl / eod_exits.len() as f64 },
            eod_pnl)?;

        // Trade size distribution
        writeln!(f)?;
        writeln!(f, "─── TRADE SIZE DISTRIBUTION ───")?;

        let mut pnls: Vec<f64> = self.trades.iter().map(|t| t.pnl_points).collect();
        pnls.sort_by(|a, b| a.partial_cmp(b).unwrap());

        if !pnls.is_empty() {
            let min = pnls.first().unwrap();
            let max = pnls.last().unwrap();
            let median = pnls[pnls.len() / 2];
            let p10 = pnls[pnls.len() / 10];
            let p90 = pnls[pnls.len() * 9 / 10];

            writeln!(f, "Min:    {:+.1} pts", min)?;
            writeln!(f, "10th %: {:+.1} pts", p10)?;
            writeln!(f, "Median: {:+.1} pts", median)?;
            writeln!(f, "90th %: {:+.1} pts", p90)?;
            writeln!(f, "Max:    {:+.1} pts", max)?;

            // Count by size buckets
            let tiny = pnls.iter().filter(|&&p| p.abs() < 3.0).count();
            let small = pnls.iter().filter(|&&p| p.abs() >= 3.0 && p.abs() < 10.0).count();
            let medium = pnls.iter().filter(|&&p| p.abs() >= 10.0 && p.abs() < 20.0).count();
            let large = pnls.iter().filter(|&&p| p.abs() >= 20.0 && p.abs() < 50.0).count();
            let huge = pnls.iter().filter(|&&p| p.abs() >= 50.0).count();

            writeln!(f)?;
            writeln!(f, "Size Buckets:")?;
            writeln!(f, "  <3 pts:     {} trades ({:.1}%)", tiny, tiny as f64 / pnls.len() as f64 * 100.0)?;
            writeln!(f, "  3-10 pts:   {} trades ({:.1}%)", small, small as f64 / pnls.len() as f64 * 100.0)?;
            writeln!(f, "  10-20 pts:  {} trades ({:.1}%)", medium, medium as f64 / pnls.len() as f64 * 100.0)?;
            writeln!(f, "  20-50 pts:  {} trades ({:.1}%)", large, large as f64 / pnls.len() as f64 * 100.0)?;
            writeln!(f, "  50+ pts:    {} trades ({:.1}%)", huge, huge as f64 / pnls.len() as f64 * 100.0)?;

            // Winners breakdown
            let winners: Vec<f64> = pnls.iter().filter(|&&p| p > 0.0).cloned().collect();
            if !winners.is_empty() {
                let big_winners = winners.iter().filter(|&&p| p >= 15.0).count();
                let max_win = winners.iter().cloned().fold(0.0_f64, f64::max);
                writeln!(f)?;
                writeln!(f, "Winners: {} total, {} over 15pts ({:.1}%), max={:.1}pts",
                    winners.len(), big_winners,
                    big_winners as f64 / winners.len() as f64 * 100.0,
                    max_win)?;
            }
        }

        // Show individual trades if small sample
        if self.trades.len() <= 10 {
            writeln!(f)?;
            writeln!(f, "─── INDIVIDUAL TRADES ───")?;
            for (i, trade) in self.trades.iter().enumerate() {
                let dir = match trade.direction {
                    ImpulseDir::Up => "LONG ",
                    ImpulseDir::Down => "SHORT",
                };
                let exit = match trade.exit_reason {
                    ExitReason::TakeProfit => "TP",
                    ExitReason::TrailingStop => "TS",
                    ExitReason::EndOfDay => "EOD",
                };
                let entry_time = trade.entry_time.with_timezone(&New_York);
                writeln!(f, "  {}. {} {} @ {:.2} → {:.2} ({}) {:+.2} pts  [LVN: {:.2}]",
                    i + 1,
                    entry_time.format("%H:%M:%S"),
                    dir,
                    trade.entry_price,
                    trade.exit_price,
                    exit,
                    trade.pnl_points,
                    trade.lvn_price)?;
            }
        }

        writeln!(f, "\n═══════════════════════════════════════════════════════════\n")?;

        Ok(())
    }
}
