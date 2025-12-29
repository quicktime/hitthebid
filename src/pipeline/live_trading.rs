//! Live Trading Module for LVN Retest Strategy
//!
//! Integrates the execution engine with signal generation for real trading.
//! Supports paper (Rithmic Demo) and live (Rithmic Live) modes.
//!
//! For testing/validation, use the `replay-test` command instead.

use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn};

use orderflow_bubbles::execution::{
    ExecutionConfig, ExecutionMode, ExecutionEngine, TradingSignal, OrderSide,
};

use super::lvn_retest::{LvnRetestConfig, LvnSignalGenerator, Direction};
use super::precompute;

/// Convert Direction to execution OrderSide
fn direction_to_side(direction: Direction) -> OrderSide {
    match direction {
        Direction::Long => OrderSide::Buy,
        Direction::Short => OrderSide::Sell,
    }
}

/// Main trading loop for paper/live trading
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
    _speed: u32,  // Not used for real trading
) -> Result<()> {
    info!("=== AUTOMATED TRADING ===");

    // Parse execution mode - only paper and live are supported
    let exec_mode = match mode.to_lowercase().as_str() {
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
        "simulation" | "sim" => {
            println!("\n⚠️  Simulation mode is deprecated.");
            println!("Use 'pipeline replay-test' instead for backtesting validation.");
            println!("\nExample:");
            println!("  ./target/release/pipeline replay-test --take-profit {} --trailing-stop {} --stop-buffer {}",
                take_profit, trailing_stop, stop_buffer);
            return Ok(());
        }
        _ => {
            warn!("Unknown mode '{}'. Use 'paper' or 'live'.", mode);
            return Ok(());
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
        max_daily_losses: 3,  // Stop after 3 losses
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

    // Connect to Rithmic
    info!("Connecting to Rithmic...");
    engine.connect().await?;
    info!("Connected!");

    // Create LVN retest config for signal generation
    let lvn_config = LvnRetestConfig {
        level_tolerance,
        retest_distance: 8.0,
        min_delta_for_absorption: min_delta,
        max_range_for_absorption: 1.5,
        stop_loss: stop_buffer,
        take_profit,
        trailing_stop,
        max_hold_bars: 300,
        rth_only: true,
        cooldown_bars: 60,
        level_cooldown_bars: 600,
        max_lvn_volume_ratio: max_lvn_ratio,
        same_day_only: false,
        min_absorption_bars: 1,
        structure_stop_buffer: stop_buffer,
        trade_start_hour: start_hour,
        trade_start_minute: start_minute,
        trade_end_hour: end_hour,
        trade_end_minute: end_minute,
    };

    info!("Config: TP={}, Trail={}, StopBuf={}, MinDelta={}, MaxRange={}",
        lvn_config.take_profit, lvn_config.trailing_stop, lvn_config.structure_stop_buffer,
        lvn_config.min_delta_for_absorption, lvn_config.max_range_for_absorption);

    // Initialize signal generator
    let mut signal_gen = LvnSignalGenerator::new(lvn_config);

    // Load cached LVN levels (for demo - in real mode this would come from live data)
    info!("Loading LVN levels from {:?}...", cache_dir);
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    // Load all LVN levels
    for day in &days {
        signal_gen.add_lvn_levels(&day.lvn_levels);
    }
    info!("Loaded LVN levels from {} days", days.len());

    // TODO: In real mode, this would be:
    // 1. Subscribe to live market data via Databento
    // 2. Process trades to build 1-second bars
    // 3. Feed bars to signal_gen.process_bar()
    // 4. Execute signals via engine.execute_signal()
    // 5. Handle fills, update trailing stops, etc.

    info!("Ready for trading. Waiting for market data...");
    info!("(Live data integration not yet implemented - use replay-test for validation)");

    // For now, just disconnect
    engine.disconnect().await?;

    Ok(())
}
