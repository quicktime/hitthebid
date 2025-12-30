//! Replay Trading Module
//!
//! Tests the live trading code path against historical data.
//! Uses the EXACT SAME LiveTrader as live trading mode.
//!
//! This validates that live trading will produce results matching expectations.

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use super::precompute;
use super::rithmic_live::{LiveConfig, LiveTrader, TradingSummary};
use super::state_machine::{StateMachineConfig, LiveDailyLevels};

/// Run replay trading test using the same LiveTrader as live mode
pub async fn run_replay(
    cache_dir: PathBuf,
    date: Option<String>,
    config: LiveConfig,
) -> Result<TradingSummary> {
    info!("=== REPLAY TRADING TEST ===");
    info!("Starting balance: ${:.2}", config.starting_balance);
    info!("Contracts: {}", config.contracts);
    if config.max_daily_losses > 0 {
        info!("Max daily losses: {}", config.max_daily_losses);
    }

    let mut trader = LiveTrader::new(config.clone());

    // Load cached data
    info!("Loading cached data from {:?}...", cache_dir);
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    info!("Loaded {} days of data", days.len());

    // Process each day - ONLY using LVN levels from PREVIOUS day (no look-ahead bias)
    let mut total_bars = 0;
    let mut total_lvn_levels = 0;

    for (i, day) in days.iter().enumerate() {
        // Before processing today's bars, load YESTERDAY's LVN levels
        // This simulates real trading where we'd run precompute overnight
        if i > 0 {
            let yesterday = &days[i - 1];
            trader.add_lvn_levels(&yesterday.lvn_levels);
            total_lvn_levels += yesterday.lvn_levels.len();
        }

        // Process today's bars
        let mut last_price = None;
        for bar in &day.bars_1s {
            total_bars += 1;
            last_price = Some(bar.close);
            let _ = trader.process_bar(bar);
        }

        // Reset daily state for next day (close any open position at EOD)
        trader.reset_for_new_day(last_price);
    }

    info!("Processed {} bars using {} LVN levels (no look-ahead)", total_bars, total_lvn_levels);

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
    println!("Sharpe Ratio:      {:.2}", summary.sharpe_ratio);
    println!("Avg Win:           {:.2} pts", summary.avg_win);
    println!("Avg Loss:          {:.2} pts", summary.avg_loss);
    println!();

    // Show P&L breakdown if costs are applied
    if summary.total_slippage > 0.0 || summary.total_commission > 0.0 {
        println!("─── P&L Breakdown ───");
        println!("Gross P&L:         {:+.2} pts (${:+.2})",
            summary.gross_pnl,
            summary.gross_pnl * config.point_value * config.contracts as f64);
        println!("Slippage:          -{:.2} pts (${:.2})",
            summary.total_slippage,
            summary.total_slippage * config.point_value * config.contracts as f64);
        println!("Commission:        ${:.2}", summary.total_commission);
        println!("Net P&L:           {:+.2} pts (${:+.2})",
            summary.net_pnl,
            summary.net_pnl * config.point_value * config.contracts as f64 - summary.total_commission);
        println!();
    } else {
        println!("Total P&L:         {:+.2} pts", summary.net_pnl);
        println!();
    }

    println!("Final Balance:     ${:.2}", summary.final_balance);
    println!("Max Drawdown:      ${:.2}", summary.max_drawdown);

    if config.max_daily_losses > 0 {
        println!();
        println!("─── Daily Loss Limit ({} losses/day) ───", config.max_daily_losses);
        println!("Days Stopped Early: {}", summary.days_stopped_early);
        println!("Signals Skipped:    {}", summary.signals_skipped);
    }

    println!("\n═══════════════════════════════════════════════════════════\n");

    Ok(summary)
}

/// Run replay with real-time state machine mode
///
/// This validates the full state machine flow:
/// 1. Detect breakouts using daily levels
/// 2. Profile impulse legs in real-time
/// 3. Extract LVNs from those impulses
/// 4. Hunt for retest with delta confirmation
/// 5. Clear LVNs after trade
pub async fn run_replay_realtime(
    cache_dir: PathBuf,
    date: Option<String>,
    config: LiveConfig,
    sm_config: StateMachineConfig,
) -> Result<TradingSummary> {
    info!("=== REPLAY REALTIME TEST (State Machine Mode) ===");
    info!("Starting balance: ${:.2}", config.starting_balance);
    info!("Contracts: {}", config.contracts);
    info!("State machine breakout threshold: {:.1} pts", sm_config.breakout_threshold);
    info!("Min impulse size: {:.1} pts", sm_config.min_impulse_size);

    let mut trader = LiveTrader::new_with_state_machine(config.clone(), sm_config);

    // Load cached data
    info!("Loading cached data from {:?}...", cache_dir);
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    info!("Loaded {} days of data", days.len());

    // Process each day
    let mut total_bars = 0;

    for (i, day) in days.iter().enumerate() {
        // Before processing today, set up daily levels from PREVIOUS day
        if i > 0 {
            let yesterday = &days[i - 1];

            // Create LiveDailyLevels from yesterday's data
            // We need to compute PDH/PDL from yesterday's bars
            let yesterday_high = yesterday.bars_1s.iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let yesterday_low = yesterday.bars_1s.iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);

            // For simplicity, use session high/low as ON levels
            // In production, you'd compute these more precisely
            let daily_levels = LiveDailyLevels {
                date: day.bars_1s.first()
                    .map(|b| b.timestamp.date_naive())
                    .unwrap_or_else(|| chrono::Utc::now().date_naive()),
                pdh: yesterday_high,
                pdl: yesterday_low,
                onh: yesterday_high, // Simplified
                onl: yesterday_low,  // Simplified
                vah: yesterday_high - (yesterday_high - yesterday_low) * 0.3, // Approx VAH
                val: yesterday_low + (yesterday_high - yesterday_low) * 0.3,  // Approx VAL
                session_high: yesterday_high,
                session_low: yesterday_low,
            };

            trader.set_daily_levels(daily_levels);
            info!("Set daily levels: PDH={:.2} PDL={:.2}", yesterday_high, yesterday_low);
        }

        // Process today's bars
        let mut last_price = None;
        for bar in &day.bars_1s {
            total_bars += 1;
            last_price = Some(bar.close);
            let _ = trader.process_bar(bar);
        }

        // Reset daily state for next day
        trader.reset_for_new_day(last_price);
    }

    info!("Processed {} bars using real-time state machine", total_bars);

    let summary = trader.summary();

    println!("\n═══════════════════════════════════════════════════════════");
    println!("         REPLAY REALTIME RESULTS (State Machine)           ");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Bars Processed:    {}", total_bars);
    println!("Total Trades:      {}", summary.total_trades);
    println!("Wins:              {} ({:.1}%)", summary.wins, summary.win_rate);
    println!("Losses:            {}", summary.losses);
    println!("Breakevens:        {}", summary.breakevens);
    println!();
    println!("Profit Factor:     {:.2}", summary.profit_factor);
    println!("Sharpe Ratio:      {:.2}", summary.sharpe_ratio);
    println!("Avg Win:           {:.2} pts", summary.avg_win);
    println!("Avg Loss:          {:.2} pts", summary.avg_loss);
    println!();
    println!("Total P&L:         {:+.2} pts", summary.net_pnl);
    println!("Final Balance:     ${:.2}", summary.final_balance);
    println!("Max Drawdown:      ${:.2}", summary.max_drawdown);

    println!("\n═══════════════════════════════════════════════════════════\n");

    Ok(summary)
}
