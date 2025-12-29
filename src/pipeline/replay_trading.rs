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

    // Load ALL LVN levels upfront (like live mode would have)
    let total_lvn_levels: usize = days.iter().map(|d| d.lvn_levels.len()).sum();
    for day in &days {
        trader.add_lvn_levels(&day.lvn_levels);
    }
    info!("Loaded {} days, {} LVN levels", days.len(), total_lvn_levels);

    // Process each bar through the SAME LiveTrader used in live mode
    let mut total_bars = 0;
    for day in &days {
        for bar in &day.bars_1s {
            total_bars += 1;
            // Just process bar - we don't need to handle TradeAction here
            // since LiveTrader tracks all stats internally
            let _ = trader.process_bar(bar);
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
    println!("Sharpe Ratio:      {:.2}", summary.sharpe_ratio);
    println!("Total P&L:         {:+.2} pts", summary.total_pnl);
    println!("Avg Win:           {:.2} pts", summary.avg_win);
    println!("Avg Loss:          {:.2} pts", summary.avg_loss);
    println!();
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
