//! Replay Trading Module
//!
//! Tests the live trading code path against historical data.
//! Uses the EXACT same trade simulation logic as the backtester.
//!
//! This serves as an integration test to verify live trading will
//! produce results matching the backtester.

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use crate::bars::Bar;
use super::lvn_retest::{
    Direction, LvnRetestConfig, LvnSignal, LvnSignalGenerator, Outcome, Trade,
};
use super::precompute;

/// Configuration for replay trading
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub lvn_config: LvnRetestConfig,
    pub starting_balance: f64,
    pub contracts: i32,
    pub point_value: f64,
    pub max_daily_losses: i32,  // Stop trading after N losses per day (0 = disabled)
}

/// Tracks an open position during replay
#[derive(Debug)]
struct OpenPosition {
    direction: Direction,
    entry_price: f64,
    entry_bar_idx: usize,
    level_price: f64,
    initial_stop: f64,
    take_profit: f64,
    trailing_stop: f64,
    highest_price: f64,
    lowest_price: f64,
    reason: String,
    entry_time: chrono::DateTime<chrono::Utc>,
}

/// Replay trading state
pub struct ReplayTrader {
    config: ReplayConfig,
    signal_gen: LvnSignalGenerator,
    pending_signal: Option<LvnSignal>,
    open_position: Option<OpenPosition>,
    trades: Vec<Trade>,
    bar_count: usize,
    running_balance: f64,
    max_drawdown: f64,
    peak_balance: f64,
    // Daily loss tracking
    current_date: Option<chrono::NaiveDate>,
    daily_losses: i32,
    daily_stopped: bool,
    days_stopped_early: u32,
    signals_skipped: u32,
}

impl ReplayTrader {
    pub fn new(config: ReplayConfig) -> Self {
        let signal_gen = LvnSignalGenerator::new(config.lvn_config.clone());
        let starting_balance = config.starting_balance;

        Self {
            config,
            signal_gen,
            pending_signal: None,
            open_position: None,
            trades: Vec::new(),
            bar_count: 0,
            running_balance: starting_balance,
            max_drawdown: 0.0,
            peak_balance: starting_balance,
            current_date: None,
            daily_losses: 0,
            daily_stopped: false,
            days_stopped_early: 0,
            signals_skipped: 0,
        }
    }

    /// Add LVN levels to track
    pub fn add_lvn_levels(&mut self, levels: &[crate::lvn::LvnLevel]) {
        self.signal_gen.add_lvn_levels(levels);
    }

    /// Process a single bar - returns trade if one completed
    pub fn process_bar(&mut self, bar: &Bar) -> Option<Trade> {
        self.bar_count += 1;
        let mut completed_trade = None;

        // Check for new day - reset daily counters
        let bar_date = bar.timestamp.date_naive();
        if self.current_date != Some(bar_date) {
            if self.daily_stopped {
                // We were stopped yesterday, count it
                self.days_stopped_early += 1;
            }
            self.current_date = Some(bar_date);
            self.daily_losses = 0;
            self.daily_stopped = false;
        }

        // Step 1: If we have a pending signal, enter on THIS bar's open
        // (Only if not stopped for the day)
        if let Some(signal) = self.pending_signal.take() {
            if self.daily_stopped {
                // Skip this signal - we're done for the day
                self.signals_skipped += 1;
                info!("SKIPPED: {} signal (max daily losses reached)", signal.direction);
                return None;
            }
            let entry_price = bar.open;
            let level_price = signal.level_price;

            // Structure-based stops (same as backtester)
            let (initial_stop, take_profit) = match signal.direction {
                Direction::Long => (
                    level_price - self.config.lvn_config.structure_stop_buffer,
                    entry_price + self.config.lvn_config.take_profit,
                ),
                Direction::Short => (
                    level_price + self.config.lvn_config.structure_stop_buffer,
                    entry_price - self.config.lvn_config.take_profit,
                ),
            };

            self.open_position = Some(OpenPosition {
                direction: signal.direction,
                entry_price,
                entry_bar_idx: self.bar_count,
                level_price,
                initial_stop,
                take_profit,
                trailing_stop: initial_stop,
                highest_price: entry_price,
                lowest_price: entry_price,
                reason: signal.reason,
                entry_time: bar.timestamp,
            });

            info!(
                "ENTRY: {} @ {:.2} | Stop: {:.2} | Target: {:.2}",
                signal.direction, entry_price, initial_stop, take_profit
            );
        }

        // Step 2: If we have an open position, update it and check for exit
        if let Some(ref mut pos) = self.open_position {
            // Update high/low water marks
            pos.highest_price = pos.highest_price.max(bar.high);
            pos.lowest_price = pos.lowest_price.min(bar.low);

            // Update trailing stop (same logic as backtester)
            let activation_distance = self.config.lvn_config.trailing_stop;
            match pos.direction {
                Direction::Long => {
                    if pos.highest_price >= pos.entry_price + activation_distance {
                        let new_trail = pos.highest_price - self.config.lvn_config.trailing_stop;
                        if new_trail > pos.trailing_stop {
                            pos.trailing_stop = new_trail;
                        }
                    }
                }
                Direction::Short => {
                    if pos.lowest_price <= pos.entry_price - activation_distance {
                        let new_trail = pos.lowest_price + self.config.lvn_config.trailing_stop;
                        if new_trail < pos.trailing_stop {
                            pos.trailing_stop = new_trail;
                        }
                    }
                }
            }

            // Check for exit
            let mut exit_price = None;
            let mut exit_type = "";

            match pos.direction {
                Direction::Long => {
                    if bar.low <= pos.trailing_stop {
                        exit_price = Some(pos.trailing_stop);
                        exit_type = "STOP";
                    } else if bar.high >= pos.take_profit {
                        exit_price = Some(pos.take_profit);
                        exit_type = "TARGET";
                    }
                }
                Direction::Short => {
                    if bar.high >= pos.trailing_stop {
                        exit_price = Some(pos.trailing_stop);
                        exit_type = "STOP";
                    } else if bar.low <= pos.take_profit {
                        exit_price = Some(pos.take_profit);
                        exit_type = "TARGET";
                    }
                }
            }

            // Check timeout
            let hold_bars = self.bar_count - pos.entry_bar_idx;
            if exit_price.is_none() && hold_bars >= self.config.lvn_config.max_hold_bars {
                exit_price = Some(bar.close);
                exit_type = "TIMEOUT";
            }

            // Process exit
            if let Some(price) = exit_price {
                let pnl_points = match pos.direction {
                    Direction::Long => price - pos.entry_price,
                    Direction::Short => pos.entry_price - price,
                };

                let outcome = if pnl_points > 0.5 {
                    Outcome::Win
                } else if pnl_points < -0.5 {
                    Outcome::Loss
                } else {
                    Outcome::Breakeven
                };

                let peak_unrealized = match pos.direction {
                    Direction::Long => pos.highest_price - pos.entry_price,
                    Direction::Short => pos.entry_price - pos.lowest_price,
                };

                let trade = Trade {
                    entry_time: pos.entry_time,
                    exit_time: bar.timestamp,
                    direction: pos.direction,
                    entry_price: pos.entry_price,
                    exit_price: price,
                    stop_loss: pos.initial_stop,
                    take_profit: pos.take_profit,
                    pnl_points,
                    peak_unrealized,
                    outcome,
                    level_price: pos.level_price,
                    hold_bars,
                    entry_reason: pos.reason.clone(),
                };

                // Update balance
                let pnl_dollars = pnl_points * self.config.point_value * self.config.contracts as f64;
                self.running_balance += pnl_dollars;
                self.peak_balance = self.peak_balance.max(self.running_balance);
                let drawdown = self.peak_balance - self.running_balance;
                self.max_drawdown = self.max_drawdown.max(drawdown);

                info!(
                    "EXIT: {} @ {:.2} | P&L: {:+.2} pts (${:+.2}) | {}",
                    exit_type, price, pnl_points, pnl_dollars,
                    match outcome {
                        Outcome::Win => "WIN",
                        Outcome::Loss => "LOSS",
                        Outcome::Breakeven => "BE",
                        Outcome::Timeout => "TIMEOUT",
                    }
                );

                // Track daily losses
                if outcome == Outcome::Loss {
                    self.daily_losses += 1;
                    if self.config.max_daily_losses > 0 && self.daily_losses >= self.config.max_daily_losses {
                        self.daily_stopped = true;
                        info!(
                            "MAX DAILY LOSSES ({}) REACHED - done for the day",
                            self.config.max_daily_losses
                        );
                    }
                }

                self.trades.push(trade.clone());
                completed_trade = Some(trade);
                self.open_position = None;
            }
        }

        // Step 3: If flat and not stopped for the day, check for new signal
        if self.open_position.is_none() && self.pending_signal.is_none() && !self.daily_stopped {
            if let Some(signal) = self.signal_gen.process_bar(bar) {
                info!(
                    "SIGNAL: {} @ {:.2} | Level: {:.2} | Delta: {}",
                    signal.direction, signal.price, signal.level_price, signal.delta
                );
                // Store signal - will enter on NEXT bar
                self.pending_signal = Some(signal);
            }
        }

        completed_trade
    }

    /// Get all completed trades
    pub fn trades(&self) -> &[Trade] {
        &self.trades
    }

    /// Get running balance
    pub fn balance(&self) -> f64 {
        self.running_balance
    }

    /// Get max drawdown
    pub fn max_drawdown(&self) -> f64 {
        self.max_drawdown
    }

    /// Check if in a position
    pub fn is_flat(&self) -> bool {
        self.open_position.is_none() && self.pending_signal.is_none()
    }

    /// Get summary statistics
    pub fn summary(&self) -> ReplaySummary {
        let total = self.trades.len() as u32;
        let wins = self.trades.iter().filter(|t| t.outcome == Outcome::Win).count() as u32;
        let losses = self.trades.iter().filter(|t| t.outcome == Outcome::Loss).count() as u32;
        let breakevens = self.trades.iter().filter(|t| t.outcome == Outcome::Breakeven).count() as u32;

        let total_pnl: f64 = self.trades.iter().map(|t| t.pnl_points).sum();
        let win_rate = if total > 0 { wins as f64 / total as f64 * 100.0 } else { 0.0 };

        let gross_profit: f64 = self.trades.iter()
            .filter(|t| t.pnl_points > 0.0)
            .map(|t| t.pnl_points)
            .sum();
        let gross_loss: f64 = self.trades.iter()
            .filter(|t| t.pnl_points < 0.0)
            .map(|t| t.pnl_points.abs())
            .sum();
        let profit_factor = if gross_loss > 0.0 { gross_profit / gross_loss } else { 0.0 };

        let avg_win = if wins > 0 {
            self.trades.iter()
                .filter(|t| t.pnl_points > 0.0)
                .map(|t| t.pnl_points)
                .sum::<f64>() / wins as f64
        } else { 0.0 };

        let avg_loss = if losses > 0 {
            self.trades.iter()
                .filter(|t| t.pnl_points < 0.0)
                .map(|t| t.pnl_points)
                .sum::<f64>() / losses as f64
        } else { 0.0 };

        // Count final day if stopped
        let days_stopped = if self.daily_stopped {
            self.days_stopped_early + 1
        } else {
            self.days_stopped_early
        };

        ReplaySummary {
            total_trades: total,
            wins,
            losses,
            breakevens,
            win_rate,
            profit_factor,
            total_pnl,
            avg_win,
            avg_loss,
            max_drawdown: self.max_drawdown,
            final_balance: self.running_balance,
            days_stopped_early: days_stopped,
            signals_skipped: self.signals_skipped,
        }
    }
}

#[derive(Debug)]
pub struct ReplaySummary {
    pub total_trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub breakevens: u32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub max_drawdown: f64,
    pub final_balance: f64,
    // Daily loss limit stats
    pub days_stopped_early: u32,
    pub signals_skipped: u32,
}

/// Run replay trading test
pub async fn run_replay(
    cache_dir: PathBuf,
    date: Option<String>,
    config: ReplayConfig,
) -> Result<ReplaySummary> {
    info!("=== REPLAY TRADING TEST ===");
    info!("Starting balance: ${:.2}", config.starting_balance);
    info!("Contracts: {}", config.contracts);
    if config.max_daily_losses > 0 {
        info!("Max daily losses: {}", config.max_daily_losses);
    }

    let max_daily_losses = config.max_daily_losses;
    let mut trader = ReplayTrader::new(config);

    // Load cached data
    info!("Loading cached data from {:?}...", cache_dir);
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    // Load ALL LVN levels upfront (like backtester)
    let total_lvn_levels: usize = days.iter().map(|d| d.lvn_levels.len()).sum();
    for day in &days {
        trader.add_lvn_levels(&day.lvn_levels);
    }
    info!("Loaded {} days, {} LVN levels", days.len(), total_lvn_levels);

    // Process each bar
    let mut total_bars = 0;
    for day in &days {
        for bar in &day.bars_1s {
            total_bars += 1;
            trader.process_bar(bar);
        }
    }

    let summary = trader.summary();

    println!("\n═══════════════════════════════════════════════════════════");
    println!("              REPLAY TRADING RESULTS                        ");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Bars Processed:    {}", total_bars);
    println!("Total Trades:      {}", summary.total_trades);
    println!("Wins:              {} ({:.1}%)", summary.wins, summary.win_rate);
    println!("Losses:            {}", summary.losses);
    println!("Breakevens:        {}", summary.breakevens);
    println!();
    println!("Profit Factor:     {:.2}", summary.profit_factor);
    println!("Total P&L:         {:+.2} pts", summary.total_pnl);
    println!("Avg Win:           {:.2} pts", summary.avg_win);
    println!("Avg Loss:          {:.2} pts", summary.avg_loss);
    println!();
    println!("Final Balance:     ${:.2}", summary.final_balance);
    println!("Max Drawdown:      ${:.2}", summary.max_drawdown);

    if max_daily_losses > 0 {
        println!();
        println!("─── Daily Loss Limit ({} losses/day) ───", max_daily_losses);
        println!("Days Stopped Early: {}", summary.days_stopped_early);
        println!("Signals Skipped:    {}", summary.signals_skipped);
    }

    println!("\n═══════════════════════════════════════════════════════════\n");

    Ok(summary)
}
