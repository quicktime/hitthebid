//! Replay Trading Module
//!
//! Tests the live trading code path against historical data.
//! Uses the EXACT SAME LiveTrader as live trading mode.
//!
//! This validates that live trading will produce results matching expectations.

use anyhow::Result;
use chrono::Timelike;
use std::path::PathBuf;
use tracing::info;

use super::bars::Bar;
use super::precompute;
use super::live_trader::{LiveConfig, LiveTrader, TradingSummary};
use super::state_machine::{StateMachineConfig, LiveDailyLevels};
use super::trades::{Trade, Side};

/// Synthesize trades from a bar for volume profile building
///
/// Creates trades that approximate the actual volume distribution within the bar.
/// This is used to feed the state machine during impulse profiling when we only
/// have bar data (from cache) instead of raw trade data.
fn synthesize_trades_from_bar(bar: &Bar) -> Vec<Trade> {
    let mut trades = Vec::new();

    // Skip bars with no volume
    if bar.volume == 0 {
        return trades;
    }

    // Create trades at key price levels (OHLC)
    // This gives a reasonable approximation of volume distribution
    // For LVN detection, we care about which prices had volume

    // Determine bar direction to allocate volume sensibly
    let is_bullish = bar.close >= bar.open;

    if is_bullish {
        // Bullish bar: buys dominate, distribute accordingly
        // More volume at low (where buyers stepped in) and close (where it ended)
        if bar.buy_volume > 0 {
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.low,
                size: bar.buy_volume / 2,
                side: Side::Buy,
                symbol: bar.symbol.clone(),
            });
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.close,
                size: bar.buy_volume / 2,
                side: Side::Buy,
                symbol: bar.symbol.clone(),
            });
        }
        if bar.sell_volume > 0 {
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.high,
                size: bar.sell_volume,
                side: Side::Sell,
                symbol: bar.symbol.clone(),
            });
        }
    } else {
        // Bearish bar: sells dominate
        // More volume at high (where sellers stepped in) and close (where it ended)
        if bar.sell_volume > 0 {
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.high,
                size: bar.sell_volume / 2,
                side: Side::Sell,
                symbol: bar.symbol.clone(),
            });
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.close,
                size: bar.sell_volume / 2,
                side: Side::Sell,
                symbol: bar.symbol.clone(),
            });
        }
        if bar.buy_volume > 0 {
            trades.push(Trade {
                ts_event: bar.timestamp,
                price: bar.low,
                size: bar.buy_volume,
                side: Side::Buy,
                symbol: bar.symbol.clone(),
            });
        }
    }

    trades
}

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
        let mut trades_fed = 0u64;

        // Track RTH session high/low for evening session levels
        let mut rth_high = f64::NEG_INFINITY;
        let mut rth_low = f64::INFINITY;
        let mut rth_levels_updated = false;

        for bar in &day.bars_1s {
            total_bars += 1;
            last_price = Some(bar.close);

            // Get the hour in ET (bars are in UTC, ET is UTC-5 or UTC-4 depending on DST)
            // For simplicity, assume UTC-5 (EST)
            let bar_hour = (bar.timestamp.hour() + 24 - 5) % 24;

            // Track RTH high/low (9:30-16:00 ET)
            if bar_hour >= 9 && bar_hour < 16 {
                rth_high = rth_high.max(bar.high);
                rth_low = rth_low.min(bar.low);
            }

            // Update levels for evening session when we cross into post-market
            // At 17:00+ (after market close), use today's RTH as the new levels
            if bar_hour >= 17 && !rth_levels_updated && rth_high > f64::NEG_INFINITY {
                let evening_levels = LiveDailyLevels {
                    date: bar.timestamp.date_naive(),
                    pdh: rth_high,
                    pdl: rth_low,
                    onh: rth_high,
                    onl: rth_low,
                    vah: rth_high - (rth_high - rth_low) * 0.3,
                    val: rth_low + (rth_high - rth_low) * 0.3,
                    session_high: rth_high,
                    session_low: rth_low,
                };
                trader.set_daily_levels(evening_levels);
                info!("Updated evening levels from RTH: PDH={:.2} PDL={:.2}", rth_high, rth_low);
                rth_levels_updated = true;
            }

            // Check if we were profiling BEFORE processing this bar
            let was_profiling = trader.is_profiling_impulse();

            // If already profiling, feed this bar's trades BEFORE process_bar()
            // This ensures trades are available when impulse completes
            if was_profiling {
                let synthetic_trades = synthesize_trades_from_bar(bar);
                for trade in &synthetic_trades {
                    trader.process_trade(trade);
                    trades_fed += 1;
                }
            }

            // Process bar (this may enter/exit profiling state)
            let _ = trader.process_bar(bar);

            // If we JUST started profiling (breakout bar), feed this bar's trades
            if !was_profiling && trader.is_profiling_impulse() {
                let synthetic_trades = synthesize_trades_from_bar(bar);
                for trade in &synthetic_trades {
                    trader.process_trade(trade);
                    trades_fed += 1;
                }
            }
        }

        if trades_fed > 0 {
            info!("Day {}: Fed {} synthetic trades during impulse profiling", day.date, trades_fed);
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
