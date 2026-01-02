//! Parameter Sweep Module
//!
//! Runs parallel parameter sweeps using Rayon for maximum performance.
//! Loads data once and tests thousands of configurations in minutes.

use anyhow::Result;
use chrono::Timelike;
use chrono_tz::America::New_York;
use rayon::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::bars::Bar;
use super::precompute::{self, DayData};
use super::state_machine::{LiveDailyLevels, StateMachineConfig};
use super::trader::{LiveConfig, LiveTrader, TradingSummary};
use super::trades::{Side, Trade};

/// Parameter configuration for sweep
#[derive(Debug, Clone)]
pub struct SweepParams {
    pub min_delta: i64,
    pub max_lvn_ratio: f64,
    pub min_impulse_size: f64,
    pub min_impulse_score: u8,
    pub take_profit: f64,
    pub trailing_stop: f64,
    pub stop_buffer: f64,
    pub breakout_threshold: f64,
    pub max_hunting_bars: usize,
}

/// Results from a single backtest run
#[derive(Debug, Clone)]
pub struct SweepResult {
    pub params: SweepParams,
    pub summary: TradingSummary,
}

/// Synthesize trades from a bar for volume profile building
fn synthesize_trades_from_bar(bar: &Bar) -> Vec<Trade> {
    let mut trades = Vec::new();

    if bar.volume == 0 {
        return trades;
    }

    let is_bullish = bar.close >= bar.open;

    if is_bullish {
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

/// Run a single backtest with given parameters
fn run_single_backtest(
    days: &[DayData],
    params: &SweepParams,
    base_config: &LiveConfig,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
) -> TradingSummary {
    let config = LiveConfig {
        symbol: base_config.symbol.clone(),
        exchange: base_config.exchange.clone(),
        contracts: base_config.contracts,
        cache_dir: base_config.cache_dir.clone(),
        take_profit: params.take_profit,
        trailing_stop: params.trailing_stop,
        stop_buffer: params.stop_buffer,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
        min_delta: params.min_delta,
        max_lvn_ratio: params.max_lvn_ratio,
        level_tolerance: base_config.level_tolerance,
        starting_balance: base_config.starting_balance,
        max_daily_losses: 0,
        daily_loss_limit: 1000.0,
        point_value: 20.0,
        slippage: 0.5,
        commission: 4.0,
        max_win_cap: 0.0,
        volatility_slippage_factor: 0.1,
        outlier_threshold: 0.0,
    };

    let sm_config = StateMachineConfig {
        breakout_threshold: params.breakout_threshold,
        min_impulse_size: params.min_impulse_size,
        min_impulse_score: params.min_impulse_score,
        max_impulse_bars: 300,
        max_hunting_bars: params.max_hunting_bars,
        max_retrace_ratio: 0.7,
        min_bars_before_switch: 60,
    };

    let mut trader = LiveTrader::new_with_state_machine(config, sm_config);

    for (i, day) in days.iter().enumerate() {
        // Set up daily levels from previous day
        if i > 0 {
            let yesterday = &days[i - 1];
            let yesterday_high = yesterday
                .bars_1s
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let yesterday_low = yesterday
                .bars_1s
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);

            let (onh, onl, vah, val) = if let Some(cached_levels) = yesterday.daily_levels.first() {
                (
                    cached_levels.onh,
                    cached_levels.onl,
                    cached_levels.vah,
                    cached_levels.val,
                )
            } else {
                let range = yesterday_high - yesterday_low;
                (
                    yesterday_high,
                    yesterday_low,
                    yesterday_high - range * 0.3,
                    yesterday_low + range * 0.3,
                )
            };

            let daily_levels = LiveDailyLevels {
                date: day
                    .bars_1s
                    .first()
                    .map(|b| b.timestamp.date_naive())
                    .unwrap_or_else(|| chrono::Utc::now().date_naive()),
                pdh: yesterday_high,
                pdl: yesterday_low,
                onh,
                onl,
                vah,
                val,
                session_high: yesterday_high,
                session_low: yesterday_low,
            };

            trader.set_daily_levels(daily_levels);
        }

        // Process bars
        let mut last_price = None;
        let mut rth_high = f64::NEG_INFINITY;
        let mut rth_low = f64::INFINITY;
        let mut rth_levels_updated = false;

        for bar in &day.bars_1s {
            last_price = Some(bar.close);

            let et_time = bar.timestamp.with_timezone(&New_York);
            let bar_hour = et_time.hour();
            let bar_minute = et_time.minute();

            let is_rth = (bar_hour > 9 || (bar_hour == 9 && bar_minute >= 30)) && bar_hour < 16;
            if is_rth {
                rth_high = rth_high.max(bar.high);
                rth_low = rth_low.min(bar.low);
            }

            if bar_hour >= 17 && !rth_levels_updated && rth_high > f64::NEG_INFINITY {
                let range = rth_high - rth_low;
                let evening_levels = LiveDailyLevels {
                    date: bar.timestamp.date_naive(),
                    pdh: rth_high,
                    pdl: rth_low,
                    onh: rth_high,
                    onl: rth_low,
                    vah: rth_high - range * 0.3,
                    val: rth_low + range * 0.3,
                    session_high: rth_high,
                    session_low: rth_low,
                };
                trader.set_daily_levels(evening_levels);
                rth_levels_updated = true;
            }

            let was_profiling = trader.is_profiling_impulse();

            if was_profiling {
                let synthetic_trades = synthesize_trades_from_bar(bar);
                for trade in &synthetic_trades {
                    trader.process_trade(trade);
                }
            }

            let _ = trader.process_bar(bar);

            if !was_profiling && trader.is_profiling_impulse() {
                let synthetic_trades = synthesize_trades_from_bar(bar);
                for trade in &synthetic_trades {
                    trader.process_trade(trade);
                }
            }
        }

        trader.reset_for_new_day(last_price);
    }

    trader.summary()
}

/// Generate all parameter combinations
pub fn generate_combinations(
    min_delta_values: &[i64],
    max_lvn_ratio_values: &[f64],
    min_impulse_size_values: &[f64],
    min_impulse_score_values: &[u8],
    take_profit_values: &[f64],
    trailing_stop_values: &[f64],
    stop_buffer_values: &[f64],
    breakout_threshold_values: &[f64],
    max_hunting_bars_values: &[usize],
) -> Vec<SweepParams> {
    let mut combinations = Vec::new();

    for &min_delta in min_delta_values {
        for &max_lvn_ratio in max_lvn_ratio_values {
            for &min_impulse_size in min_impulse_size_values {
                for &min_impulse_score in min_impulse_score_values {
                    for &take_profit in take_profit_values {
                        for &trailing_stop in trailing_stop_values {
                            for &stop_buffer in stop_buffer_values {
                                for &breakout_threshold in breakout_threshold_values {
                                    for &max_hunting_bars in max_hunting_bars_values {
                                        combinations.push(SweepParams {
                                            min_delta,
                                            max_lvn_ratio,
                                            min_impulse_size,
                                            min_impulse_score,
                                            take_profit,
                                            trailing_stop,
                                            stop_buffer,
                                            breakout_threshold,
                                            max_hunting_bars,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    combinations
}

/// Run parameter sweep in parallel
pub fn run_sweep(
    cache_dir: PathBuf,
    output_file: PathBuf,
    combinations: Vec<SweepParams>,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
) -> Result<Vec<SweepResult>> {
    println!("Loading cached data from {:?}...", cache_dir);
    let start = std::time::Instant::now();
    let days = precompute::load_all_cached(&cache_dir, None)?;
    println!(
        "Loaded {} days in {:.1}s",
        days.len(),
        start.elapsed().as_secs_f64()
    );

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    let base_config = LiveConfig {
        symbol: "NQ".to_string(),
        exchange: "CME".to_string(),
        contracts: 1,
        cache_dir: cache_dir.clone(),
        take_profit: 30.0,
        trailing_stop: 6.0,
        stop_buffer: 2.0,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
        min_delta: 25,
        max_lvn_ratio: 0.25,
        level_tolerance: 2.0,
        starting_balance: 50000.0,
        max_daily_losses: 0,
        daily_loss_limit: 1000.0,
        point_value: 20.0,
        slippage: 0.5,
        commission: 4.0,
        max_win_cap: 0.0,
        volatility_slippage_factor: 0.1,
        outlier_threshold: 0.0,
    };

    let total = combinations.len();
    println!(
        "\nRunning {} parameter combinations in parallel...",
        total
    );

    let completed = AtomicUsize::new(0);
    let start = std::time::Instant::now();

    // Run in parallel using rayon
    let results: Vec<SweepResult> = combinations
        .par_iter()
        .map(|params| {
            let summary = run_single_backtest(
                &days,
                params,
                &base_config,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            );

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 50 == 0 || done == total {
                let elapsed = start.elapsed().as_secs_f64();
                let rate = done as f64 / elapsed;
                let eta = (total - done) as f64 / rate;
                eprint!(
                    "\r[{}/{}] {:.1}/s, ETA: {:.0}s       ",
                    done, total, rate, eta
                );
            }

            SweepResult {
                params: params.clone(),
                summary,
            }
        })
        .collect();

    eprintln!();

    // Write results to CSV
    let mut file = std::fs::File::create(&output_file)?;
    writeln!(
        file,
        "min_delta,max_lvn_ratio,min_impulse_size,min_impulse_score,take_profit,trailing_stop,stop_buffer,breakout_threshold,max_hunting_bars,total_trades,wins,losses,breakevens,win_rate,profit_factor,sharpe_ratio,avg_win,avg_loss,total_pnl,max_drawdown,rr_ratio,expectancy"
    )?;

    for result in &results {
        let p = &result.params;
        let s = &result.summary;

        let rr_ratio = if s.avg_loss != 0.0 {
            s.avg_win / s.avg_loss.abs()
        } else {
            0.0
        };

        let expectancy = if s.total_trades > 0 {
            let win_pct = s.wins as f64 / s.total_trades as f64;
            let loss_pct = s.losses as f64 / s.total_trades as f64;
            (win_pct * s.avg_win) - (loss_pct * s.avg_loss.abs())
        } else {
            0.0
        };

        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
            p.min_delta,
            p.max_lvn_ratio,
            p.min_impulse_size,
            p.min_impulse_score,
            p.take_profit,
            p.trailing_stop,
            p.stop_buffer,
            p.breakout_threshold,
            p.max_hunting_bars,
            s.total_trades,
            s.wins,
            s.losses,
            s.breakevens,
            s.win_rate,
            s.profit_factor,
            s.sharpe_ratio,
            s.avg_win,
            s.avg_loss,
            s.net_pnl,
            s.max_drawdown,
            rr_ratio,
            expectancy
        )?;
    }

    println!("\nResults written to {:?}", output_file);

    // Print summary
    let profitable: Vec<_> = results
        .iter()
        .filter(|r| r.summary.net_pnl > 0.0 && r.summary.total_trades >= 10)
        .collect();

    println!("\n=== SWEEP SUMMARY ===");
    println!("Total combinations: {}", total);
    println!(
        "Profitable (>=10 trades): {} ({:.1}%)",
        profitable.len(),
        profitable.len() as f64 / total as f64 * 100.0
    );

    if !profitable.is_empty() {
        let mut by_sharpe = profitable.clone();
        by_sharpe.sort_by(|a, b| {
            b.summary
                .sharpe_ratio
                .partial_cmp(&a.summary.sharpe_ratio)
                .unwrap()
        });

        println!("\nTop 10 by Sharpe Ratio:");
        for (i, r) in by_sharpe.iter().take(10).enumerate() {
            let p = &r.params;
            let s = &r.summary;
            println!(
                "  {}. Sharpe={:.2} PF={:.2} P&L={:+.1} Trades={} WR={:.1}%",
                i + 1,
                s.sharpe_ratio,
                s.profit_factor,
                s.net_pnl,
                s.total_trades,
                s.win_rate
            );
            println!(
                "     d={} tr={} tp={} sb={} hunt={}",
                p.min_delta, p.trailing_stop, p.take_profit, p.stop_buffer, p.max_hunting_bars
            );
        }
    }

    let elapsed = start.elapsed();
    println!(
        "\nCompleted in {:.1}s ({:.1} tests/second)",
        elapsed.as_secs_f64(),
        total as f64 / elapsed.as_secs_f64()
    );

    Ok(results)
}
