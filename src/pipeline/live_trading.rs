//! Live Trading Module for LVN Retest Strategy
//!
//! Integrates the execution engine with signal generation from paper_trading.
//! Supports simulation, paper (Rithmic Demo), and live (Rithmic Live) modes.

use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn};

use orderflow_bubbles::execution::{
    ExecutionConfig, ExecutionMode, ExecutionEngine, TradingSignal, OrderSide,
};

use super::paper_trading::{PaperConfig, PaperTradingState, Direction};
use super::precompute;

/// Convert paper trading Direction to execution OrderSide
fn direction_to_side(direction: Direction) -> OrderSide {
    match direction {
        Direction::Long => OrderSide::Buy,
        Direction::Short => OrderSide::Sell,
    }
}

/// Main trading loop
#[allow(clippy::too_many_arguments)]
pub async fn run_trading(
    mode: String,
    contracts: i32,
    daily_loss_limit: f64,
    take_profit: f64,
    trailing_stop: f64,
    stop_buffer: f64,
    cache_dir: PathBuf,
    date: Option<String>,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
    min_delta: i64,
    max_lvn_ratio: f64,
    level_tolerance: f64,
    starting_balance: f64,
) -> Result<()> {
    info!("=== AUTOMATED TRADING ===");

    // Parse execution mode
    let exec_mode = match mode.to_lowercase().as_str() {
        "simulation" | "sim" => ExecutionMode::Simulation,
        "paper" => ExecutionMode::Paper,
        "live" => {
            // Require confirmation for live mode
            println!("\n⚠️  WARNING: LIVE TRADING MODE ⚠️");
            println!("This will execute real trades with real money.");
            println!("Type 'CONFIRM' to proceed or anything else to cancel:");

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if input.trim() != "CONFIRM" {
                println!("Live trading cancelled.");
                return Ok(());
            }

            ExecutionMode::Live
        }
        _ => {
            warn!("Unknown mode '{}', defaulting to simulation", mode);
            ExecutionMode::Simulation
        }
    };

    info!("Mode: {:?}", exec_mode);
    info!("Contracts: {}", contracts);
    info!("Daily loss limit: {} pts (${:.2})", daily_loss_limit, daily_loss_limit * 20.0);
    info!("TP: {} pts, Trail: {} pts, Stop buffer: {} pts", take_profit, trailing_stop, stop_buffer);
    info!("Trading hours: {:02}:{:02} - {:02}:{:02} ET", start_hour, start_minute, end_hour, end_minute);

    // Load Rithmic credentials from environment
    let rithmic_user = std::env::var("RITHMIC_USER").unwrap_or_default();
    let rithmic_fcm_id = std::env::var("RITHMIC_FCM_ID").unwrap_or_default();
    let rithmic_ib_id = std::env::var("RITHMIC_IB_ID").unwrap_or_default();
    let rithmic_system = std::env::var("RITHMIC_SYSTEM").unwrap_or_else(|_| "Rithmic Paper Trading".to_string());
    let rithmic_env = std::env::var("RITHMIC_ENV").unwrap_or_else(|_| "Demo".to_string());

    // Create execution config
    let exec_config = ExecutionConfig {
        mode: exec_mode,
        symbol: "NQ.c.0".to_string(),
        exchange: "CME".to_string(),
        max_position_size: contracts,
        daily_loss_limit,
        take_profit,
        trailing_stop,
        stop_buffer,
        point_value: 20.0,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
        rithmic_env,
        rithmic_user,
        rithmic_fcm_id,
        rithmic_ib_id,
        rithmic_system,
    };

    // Create execution engine
    let mut engine = ExecutionEngine::with_balance(exec_config, starting_balance);

    // Connect to Rithmic (or simulate connection)
    info!("Connecting...");
    engine.connect().await?;
    info!("Connected!");

    // Create paper trading config for signal generation
    let paper_config = PaperConfig {
        level_tolerance,
        retest_distance: 8.0,
        min_delta,
        max_range: 2.5,
        take_profit,
        trailing_stop,
        stop_buffer,
        max_lvn_ratio,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
    };

    // Initialize paper trading state for signal detection
    let mut signal_state = PaperTradingState::new(paper_config);

    // Load cached data
    info!("Loading cached data from {:?}...", cache_dir);
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    info!("Loaded {} days of data", days.len());

    // Process each day
    let mut total_bars = 0;
    let mut total_signals = 0;

    for day in days {
        info!("Processing day: {}", day.date);

        // Add LVN levels for this day
        signal_state.add_lvn_levels(&day.lvn_levels);
        info!("  Added {} LVN levels", day.lvn_levels.len());

        // Process bars
        for bar in &day.bars_1s {
            total_bars += 1;

            // Check for signal from paper trading logic
            if let Some(signal) = signal_state.process_bar(bar) {
                total_signals += 1;

                info!(
                    "🚨 SIGNAL: {} @ {:.2} | Level: {:.2} | Delta: {}",
                    signal.direction, signal.price, signal.level_price, signal.delta
                );

                // Skip if daily loss limit hit
                if engine.is_daily_limit_hit() {
                    warn!("Daily loss limit reached, skipping signal");
                    continue;
                }

                // Convert to trading signal and execute
                let trading_signal = TradingSignal {
                    side: direction_to_side(signal.direction),
                    lvn_level: signal.level_price,
                    current_price: signal.price,
                    delta: signal.delta as f64,
                };

                match engine.execute_signal(&trading_signal, contracts).await {
                    Ok(bracket_id) => {
                        info!("Order submitted: bracket {}", bracket_id);
                    }
                    Err(e) => {
                        warn!("Failed to execute signal: {}", e);
                    }
                }
            }

            // Update trailing stops on open positions
            if !engine.position_manager().is_flat() {
                if let Err(e) = engine.update_trailing_stops(bar.close).await {
                    warn!("Failed to update trailing stops: {}", e);
                }

                // Check for exit triggers (simulation mode)
                let exits = engine.check_exit_triggers(bar.close);
                for (bracket_id, fill_price, exit_type) in exits {
                    engine.process_exit_fill(bracket_id, fill_price, &exit_type);
                }
            }

            // Periodic status update
            if total_bars % 3600 == 0 {
                engine.print_status();
            }
        }

        // Clear levels at end of day
        signal_state.clear_levels();

        // Print daily summary
        engine.print_status();
    }

    // Final summary
    println!("\n═══════════════════════════════════════════════════════════");
    println!("              TRADING SESSION COMPLETE                      ");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Bars Processed:    {}", total_bars);
    println!("Signals Generated: {}", total_signals);
    println!();

    let pm = engine.position_manager();
    let daily = pm.daily_summary();

    println!("Trades Executed:   {}", daily.trade_count);
    println!("Wins:              {} ({:.1}%)",
        daily.wins,
        if daily.trade_count > 0 { daily.wins as f64 / daily.trade_count as f64 * 100.0 } else { 0.0 }
    );
    println!("Losses:            {}", daily.losses);
    println!("Total P&L:         {:+.2} pts (${:.2})", daily.gross_pnl, pm.daily_pnl_dollars());
    println!();
    println!("Final Balance:     ${:.2}", pm.running_balance());
    println!("Max Drawdown:      ${:.2}", daily.max_drawdown);

    println!("\n═══════════════════════════════════════════════════════════\n");

    // Disconnect
    engine.disconnect().await?;

    info!("Trading session complete!");

    Ok(())
}
