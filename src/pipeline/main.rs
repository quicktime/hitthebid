mod trades;
mod bars;
mod levels;
mod impulse;
mod lvn;
mod supabase;
mod replay;
mod backtest;
mod market_state;
mod three_element_backtest;
mod precompute;
mod lvn_retest;
mod paper_trading;
mod monte_carlo;
mod live_trading;
mod replay_trading;
mod live_trader;
mod ib_live;
mod databento_ib_live;
mod state_machine;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser, Debug)]
#[command(name = "pipeline")]
#[command(about = "NQ futures backtesting & replay data pipeline")]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Print verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Process trade data and export to Parquet/Supabase
    Process {
        /// Path to data directory containing .zst files
        #[arg(short, long, default_value = "data/NQ_11_23_2025-12_23_2025")]
        data_dir: PathBuf,

        /// Output directory for Parquet files
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Skip Supabase upload (local processing only)
        #[arg(long)]
        no_upload: bool,
    },

    /// Replay historical trades through production ProcessingState
    Replay {
        /// Path to data directory containing .zst files
        #[arg(short, long, default_value = "data/NQ_11_23_2025-12_23_2025")]
        data_dir: PathBuf,

        /// Output directory for Parquet files
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,
    },

    /// Backtest trading strategy on historical signals
    Backtest {
        /// Path to data directory containing .zst files
        #[arg(short, long, default_value = "data/NQ_11_23_2025-12_23_2025")]
        data_dir: PathBuf,

        /// Output directory for results
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Stop loss in points
        #[arg(long, default_value = "10.0")]
        stop_loss: f64,

        /// Take profit in points
        #[arg(long, default_value = "20.0")]
        take_profit: f64,

        /// Maximum hold time in seconds
        #[arg(long, default_value = "300")]
        max_hold: u64,

        /// Only trade during RTH (9:30 AM - 4:00 PM ET)
        #[arg(long, default_value = "true")]
        rth_only: bool,

        /// Minimum confluence score (2-4)
        #[arg(long, default_value = "2")]
        min_confluence: u8,

        /// Only trade at key levels (POC, VAH, VAL, PDH, PDL)
        #[arg(long)]
        key_levels_only: bool,
    },

    /// Analyze historical data from Supabase (run backtest on cloud data)
    Analyze {
        /// Output directory for results
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,

        /// Stop loss in points
        #[arg(long, default_value = "10.0")]
        stop_loss: f64,

        /// Take profit in points
        #[arg(long, default_value = "20.0")]
        take_profit: f64,

        /// Maximum hold time in seconds
        #[arg(long, default_value = "300")]
        max_hold: u64,

        /// Include ETH (extended trading hours) - by default only RTH
        #[arg(long)]
        include_eth: bool,

        /// Minimum confluence score (2-4)
        #[arg(long, default_value = "2")]
        min_confluence: u8,

        /// Only trade at key levels (POC, VAH, VAL, PDH, PDL)
        #[arg(long)]
        key_levels_only: bool,
    },

    /// Three-Element backtest (Market State + Location + Aggression)
    ThreeElement {
        /// Path to data directory containing .zst files
        #[arg(short, long, default_value = "data/NQ_11_23_2025-12_23_2025")]
        data_dir: PathBuf,

        /// Output directory for results
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,

        /// Cache directory for precomputed data (uses cache if available)
        #[arg(short, long)]
        cache_dir: Option<PathBuf>,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Lookback bars for market state detection (in 1-second bars)
        #[arg(long, default_value = "60")]
        lookback: usize,

        /// Level tolerance in points for "at level" detection (1 point = tighter)
        #[arg(long, default_value = "1.0")]
        level_tolerance: f64,

        /// Mean Reversion stop loss (points)
        #[arg(long, default_value = "6.0")]
        mr_stop_loss: f64,

        /// Mean Reversion take profit (points)
        #[arg(long, default_value = "12.0")]
        mr_take_profit: f64,

        /// Trend Continuation stop loss (points)
        #[arg(long, default_value = "5.0")]
        tc_stop_loss: f64,

        /// Trend Continuation take profit (points)
        #[arg(long, default_value = "30.0")]
        tc_take_profit: f64,

        /// Trend Continuation trailing stop (points)
        #[arg(long, default_value = "5.0")]
        tc_trailing_stop: f64,

        /// Delta momentum threshold for aggression detection (higher = fewer signals)
        #[arg(long, default_value = "500")]
        delta_threshold: i64,

        /// Delta lookback bars for momentum calculation (in 1-second bars)
        #[arg(long, default_value = "60")]
        delta_lookback: usize,

        /// Imbalance ratio threshold (e.g., 2.0 = 2:1)
        #[arg(long, default_value = "2.0")]
        imbalance_ratio: f64,

        /// Only trade during RTH (9:30 AM - 4:00 PM ET)
        #[arg(long, default_value = "true")]
        rth_only: bool,

        /// Don't use captured signals, only detect from bars
        #[arg(long)]
        no_signals: bool,

        /// Global cooldown between trades in seconds (default: 600 = 10 min)
        #[arg(long, default_value = "600")]
        global_cooldown: usize,

        /// Per-level cooldown in seconds (default: 1800 = 30 min)
        #[arg(long, default_value = "1800")]
        level_cooldown: usize,

        /// Market state delta threshold for imbalanced detection (higher = more balanced states)
        #[arg(long, default_value = "2000")]
        ms_delta_threshold: i64,
    },

    /// Precompute signals and cache for faster backtesting
    Precompute {
        /// Path to data directory containing .zst files
        #[arg(short, long, default_value = "data/NQ_11_23_2025-12_23_2025")]
        data_dir: PathBuf,

        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,
    },

    /// LVN Retest Strategy - focused backtester for LVN pullback setups
    LvnRetest {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Level tolerance in points
        #[arg(long, default_value = "2.0")]
        level_tolerance: f64,

        /// Retest distance - how far price must move away to arm level (points)
        #[arg(long, default_value = "8.0")]
        retest_distance: f64,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "100")]
        min_delta: i64,

        /// Maximum range for absorption (points)
        #[arg(long, default_value = "1.5")]
        max_range: f64,

        /// Stop loss in points
        #[arg(long, default_value = "4.0")]
        stop_loss: f64,

        /// Take profit in points
        #[arg(long, default_value = "8.0")]
        take_profit: f64,

        /// Trailing stop distance (points)
        #[arg(long, default_value = "4.0")]
        trailing_stop: f64,

        /// Only trade during RTH
        #[arg(long, default_value = "true")]
        rth_only: bool,

        /// Max LVN volume ratio (lower = thinner = higher quality)
        #[arg(long, default_value = "0.15")]
        max_lvn_ratio: f64,

        /// Only use same-day LVNs (freshness filter)
        #[arg(long, default_value = "false")]
        same_day_only: bool,

        /// Structure stop buffer - points beyond LVN level (tighter = better R:R)
        #[arg(long, default_value = "2.0")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "16")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Exit on aggressive reverse flow (for Apex eval - minimizes peak ratio)
        #[arg(long, default_value = "false")]
        reverse_exit: bool,

        /// Delta threshold for reverse aggression exit
        #[arg(long, default_value = "50")]
        reverse_delta: i64,
    },

    /// Paper trade validation - replay historical data to validate strategy
    PaperTrade {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache")]
        cache_dir: PathBuf,

        /// State file to persist paper trading state
        #[arg(long, default_value = "paper_trading_state.json")]
        state_file: PathBuf,

        /// Log file for signals and trades
        #[arg(long, default_value = "paper_trading.log")]
        log_file: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Level tolerance in points
        #[arg(long, default_value = "2.5")]
        level_tolerance: f64,

        /// Minimum delta for signal
        #[arg(long, default_value = "75")]
        min_delta: i64,

        /// Maximum range for signal
        #[arg(long, default_value = "2.5")]
        max_range: f64,

        /// Take profit in points
        #[arg(long, default_value = "35")]
        take_profit: f64,

        /// Trailing stop distance
        #[arg(long, default_value = "12")]
        trailing_stop: f64,

        /// Structure stop buffer
        #[arg(long, default_value = "0.5")]
        stop_buffer: f64,

        /// Max LVN volume ratio
        #[arg(long, default_value = "0.15")]
        max_lvn_ratio: f64,

        /// Trading start hour (ET)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET)
        #[arg(long, default_value = "11")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Replay speed multiplier (1 = realtime, 0 = as fast as possible)
        #[arg(long, default_value = "0")]
        speed: u32,

        /// Show live status updates
        #[arg(long)]
        live_status: bool,
    },

    /// Monte Carlo simulation for Apex eval survival
    MonteCarlo {
        /// Number of simulations to run
        #[arg(short, long, default_value = "100000")]
        simulations: usize,
    },

    /// Monte Carlo simulation for Elite Trader Funding Static DD eval
    MonteCarloEtf {
        /// Number of simulations to run
        #[arg(short, long, default_value = "100000")]
        simulations: usize,
    },

    /// Automated trading with Rithmic API (paper or live)
    Trade {
        /// Trading mode: simulation, paper, or live
        #[arg(long, default_value = "simulation")]
        mode: String,

        /// Number of contracts to trade
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Daily loss limit in points (stops trading when reached)
        #[arg(long, default_value = "100")]
        daily_loss_limit: f64,

        /// Take profit in points (set high to let trailing stop work)
        #[arg(long, default_value = "500")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "6")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Cache directory for precomputed data (for replay mode)
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format) for replay
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "11")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "60")]
        min_delta: i64,

        /// Max LVN volume ratio (quality filter)
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3")]
        level_tolerance: f64,

        /// Starting balance for prop firm tracking
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Replay speed (1 = realtime, 20 = 20x speed, 0 = max speed)
        #[arg(long, default_value = "0")]
        speed: u32,
    },

    /// Replay test - validates live trading code against historical data
    ReplayTest {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Take profit in points
        #[arg(long, default_value = "30")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "6")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "11")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "60")]
        min_delta: i64,

        /// Maximum LVN volume ratio (lower = thinner = higher quality)
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3.0")]
        level_tolerance: f64,

        /// Starting balance
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Max losing trades per day (0 = disabled)
        #[arg(long, default_value = "0")]
        max_daily_losses: i32,

        /// Slippage per trade in points (applied to both entry and exit)
        #[arg(long, default_value = "0.0")]
        slippage: f64,

        /// Commission per round-trip in dollars
        #[arg(long, default_value = "0.0")]
        commission: f64,
    },

    /// Replay test with real-time state machine (no look-ahead bias)
    ReplayRealtime {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Take profit in points (set very high to use trailing stop only)
        #[arg(long, default_value = "500")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "4")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "16")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal (lower for 1-second bars)
        #[arg(long, default_value = "5")]
        min_delta: i64,

        /// Maximum LVN volume ratio (lower = thinner = higher quality)
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3.0")]
        level_tolerance: f64,

        /// Starting balance
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Breakout threshold in points
        #[arg(long, default_value = "2.0")]
        breakout_threshold: f64,

        /// Minimum impulse size in points
        #[arg(long, default_value = "15.0")]
        min_impulse_size: f64,

        /// Maximum impulse bars (1s bars, default 600 = 10 min)
        #[arg(long, default_value = "600")]
        max_impulse_bars: usize,

        /// Maximum hunting bars (1s bars, default 1800 = 30 min)
        #[arg(long, default_value = "1800")]
        max_hunting_bars: usize,

        /// Maximum retrace ratio before impulse invalidated (0.7 = 70%)
        #[arg(long, default_value = "0.7")]
        max_retrace_ratio: f64,

        /// Minimum impulse score (out of 5) to qualify (lowered for 1s bars)
        #[arg(long, default_value = "3")]
        min_impulse_score: u8,
    },

    /// Live trading (paper or live mode) - uses IB Gateway
    Live {
        /// Trading mode: "paper" or "live"
        #[arg(long, default_value = "paper")]
        mode: String,

        /// Symbol to trade (without exchange suffix)
        #[arg(long, default_value = "NQ")]
        symbol: String,

        /// Exchange
        #[arg(long, default_value = "CME")]
        exchange: String,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points (set very high to use trailing stop only)
        #[arg(long, default_value = "500")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "4")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "16")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "5")]
        min_delta: i64,

        /// Maximum LVN volume ratio
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3.0")]
        level_tolerance: f64,

        /// Starting balance for tracking
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Max losing trades per day (0 = disabled)
        #[arg(long, default_value = "3")]
        max_daily_losses: i32,

        /// Daily loss limit in points
        #[arg(long, default_value = "100")]
        daily_loss_limit: f64,

        /// Slippage per trade in points (for cost estimation)
        #[arg(long, default_value = "0.0")]
        slippage: f64,

        /// Commission per round-trip in dollars
        #[arg(long, default_value = "0.0")]
        commission: f64,
    },

    /// Test Interactive Brokers connection
    IbTest,

    /// Live trading via Interactive Brokers (paper or live)
    IbLive {
        /// Trading mode: "paper" or "live"
        #[arg(long, default_value = "paper")]
        mode: String,

        /// TWS/Gateway host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// TWS/Gateway port (7497 for paper, 7496 for live)
        #[arg(long, default_value = "7497")]
        port: u16,

        /// Client ID (must be unique per connection)
        #[arg(long, default_value = "1")]
        client_id: i32,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points
        #[arg(long, default_value = "30")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "6")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "11")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "60")]
        min_delta: i64,

        /// Maximum LVN volume ratio
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3.0")]
        level_tolerance: f64,

        /// Starting balance for tracking
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Max losing trades per day (0 = disabled)
        #[arg(long, default_value = "3")]
        max_daily_losses: i32,

        /// Daily loss limit in points
        #[arg(long, default_value = "100")]
        daily_loss_limit: f64,
    },

    /// Test IB market data subscription (works with delayed data)
    IbDataTest {
        /// Symbol to test (default: AAPL stock)
        #[arg(long, default_value = "AAPL")]
        symbol: String,

        /// Duration in seconds to collect bars
        #[arg(long, default_value = "30")]
        duration: u64,
    },

    /// Test IB NQ futures data specifically
    IbFuturesTest {
        /// Duration in seconds to collect bars
        #[arg(long, default_value = "30")]
        duration: u64,
    },

    /// IB polling mode (uses historical data, no realtime subscription needed)
    IbPolling {
        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points
        #[arg(long, default_value = "30")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "6")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,
    },

    /// Live trading with Databento data + IB execution (production mode)
    DatabentoIbLive {
        /// Trading mode: "paper" or "live"
        #[arg(long, default_value = "paper")]
        mode: String,

        /// IB Client ID (must be unique per connection)
        #[arg(long, default_value = "2")]
        client_id: i32,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points
        #[arg(long, default_value = "30")]
        take_profit: f64,

        /// Trailing stop distance in points
        #[arg(long, default_value = "6")]
        trailing_stop: f64,

        /// Stop buffer beyond LVN level in points
        #[arg(long, default_value = "1.5")]
        stop_buffer: f64,

        /// Trading start hour (ET, 24h format)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading start minute
        #[arg(long, default_value = "30")]
        start_minute: u32,

        /// Trading end hour (ET, 24h format)
        #[arg(long, default_value = "11")]
        end_hour: u32,

        /// Trading end minute
        #[arg(long, default_value = "0")]
        end_minute: u32,

        /// Minimum delta for absorption signal
        #[arg(long, default_value = "60")]
        min_delta: i64,

        /// Maximum LVN volume ratio
        #[arg(long, default_value = "0.4")]
        max_lvn_ratio: f64,

        /// Level tolerance in points
        #[arg(long, default_value = "3.0")]
        level_tolerance: f64,

        /// Starting balance for tracking
        #[arg(long, default_value = "30000")]
        starting_balance: f64,

        /// Max losing trades per day (0 = disabled)
        #[arg(long, default_value = "3")]
        max_daily_losses: i32,

        /// Daily loss limit in points
        #[arg(long, default_value = "100")]
        daily_loss_limit: f64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args = Args::parse();

    // Set up logging with filter to reduce noise from orderflow_bubbles processing
    // The processing module logs every bubble creation at INFO level which is too verbose
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        // Only show warnings from orderflow_bubbles, INFO from pipeline
        EnvFilter::new("pipeline=info,orderflow_bubbles=warn")
    };

    fmt()
        .with_env_filter(filter)
        .init();

    match args.command {
        Commands::Process { data_dir, output_dir, date, no_upload } => {
            run_process(data_dir, output_dir, date, no_upload).await?;
        }
        Commands::Replay { data_dir, output_dir, date } => {
            run_replay(data_dir, output_dir, date)?;
        }
        Commands::Backtest {
            data_dir, output_dir, date,
            stop_loss, take_profit, max_hold,
            rth_only, min_confluence, key_levels_only,
        } => {
            run_backtest(
                data_dir, output_dir, date,
                stop_loss, take_profit, max_hold,
                rth_only, min_confluence, key_levels_only,
            )?;
        }
        Commands::Analyze {
            output_dir,
            stop_loss, take_profit, max_hold,
            include_eth, min_confluence, key_levels_only,
        } => {
            run_analyze(
                output_dir,
                stop_loss, take_profit, max_hold,
                !include_eth, min_confluence, key_levels_only,
            ).await?;
        }
        Commands::ThreeElement {
            data_dir, output_dir, cache_dir, date,
            lookback, level_tolerance,
            mr_stop_loss, mr_take_profit,
            tc_stop_loss, tc_take_profit, tc_trailing_stop,
            delta_threshold, delta_lookback, imbalance_ratio,
            rth_only, no_signals,
            global_cooldown, level_cooldown, ms_delta_threshold,
        } => {
            run_three_element(
                data_dir, output_dir, cache_dir, date,
                lookback, level_tolerance,
                mr_stop_loss, mr_take_profit,
                tc_stop_loss, tc_take_profit, tc_trailing_stop,
                delta_threshold, delta_lookback, imbalance_ratio,
                rth_only, !no_signals,
                global_cooldown, level_cooldown, ms_delta_threshold,
            )?;
        }
        Commands::Precompute { data_dir, cache_dir, date } => {
            run_precompute(data_dir, cache_dir, date)?;
        }
        Commands::LvnRetest {
            cache_dir, date,
            level_tolerance, retest_distance,
            min_delta, max_range,
            stop_loss, take_profit, trailing_stop,
            rth_only, max_lvn_ratio, same_day_only, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            reverse_exit, reverse_delta,
        } => {
            run_lvn_retest(
                cache_dir, date,
                level_tolerance, retest_distance,
                min_delta, max_range,
                stop_loss, take_profit, trailing_stop,
                rth_only, max_lvn_ratio, same_day_only, stop_buffer,
                start_hour, start_minute, end_hour, end_minute,
                reverse_exit, reverse_delta,
            )?;
        }
        Commands::PaperTrade {
            cache_dir, state_file, log_file, date,
            level_tolerance, min_delta, max_range,
            take_profit, trailing_stop, stop_buffer, max_lvn_ratio,
            start_hour, start_minute, end_hour, end_minute,
            speed, live_status,
        } => {
            run_paper_trade(
                cache_dir, state_file, log_file, date,
                level_tolerance, min_delta, max_range,
                take_profit, trailing_stop, stop_buffer, max_lvn_ratio,
                start_hour, start_minute, end_hour, end_minute,
                speed, live_status,
            )?;
        }
        Commands::MonteCarlo { simulations: _ } => {
            monte_carlo::run_monte_carlo();
        }
        Commands::MonteCarloEtf { simulations: _ } => {
            monte_carlo::run_etf_monte_carlo();
        }
        Commands::Trade {
            mode, contracts, daily_loss_limit,
            take_profit, trailing_stop, stop_buffer,
            cache_dir, date,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, speed,
        } => {
            run_trade(
                mode, contracts, daily_loss_limit,
                take_profit, trailing_stop, stop_buffer,
                cache_dir, date,
                start_hour, start_minute, end_hour, end_minute,
                min_delta, max_lvn_ratio, level_tolerance,
                starting_balance, speed,
            ).await?;
        }
        Commands::ReplayTest {
            cache_dir, date,
            contracts, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses,
            slippage, commission,
        } => {
            // Use same LiveConfig as live trading - validates exact same code path
            let config = live_trader::LiveConfig {
                symbol: "NQ".to_string(),  // Not used in replay
                exchange: "CME".to_string(), // Not used in replay
                contracts,
                cache_dir: cache_dir.clone(),
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
                min_delta,
                max_lvn_ratio,
                level_tolerance,
                starting_balance,
                max_daily_losses,
                daily_loss_limit: 1000.0, // High limit for replay
                point_value: 20.0,
                slippage,
                commission,
            };

            replay_trading::run_replay(cache_dir, date, config).await?;
        }
        Commands::ReplayRealtime {
            cache_dir, date,
            contracts, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance,
            breakout_threshold, min_impulse_size,
            max_impulse_bars, max_hunting_bars, max_retrace_ratio,
            min_impulse_score,
        } => {
            let config = live_trader::LiveConfig {
                symbol: "NQ".to_string(),
                exchange: "CME".to_string(),
                contracts,
                cache_dir: cache_dir.clone(),
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
                min_delta,
                max_lvn_ratio,
                level_tolerance,
                starting_balance,
                max_daily_losses: 0, // Not used in realtime mode
                daily_loss_limit: 1000.0,
                point_value: 20.0,
                slippage: 0.0,
                commission: 0.0,
            };

            let sm_config = state_machine::StateMachineConfig {
                breakout_threshold,
                max_impulse_bars,
                min_impulse_size,
                max_hunting_bars,
                min_impulse_score,
                max_retrace_ratio,
            };

            replay_trading::run_replay_realtime(cache_dir, date, config, sm_config).await?;
        }
        Commands::Live {
            mode, symbol, exchange, contracts, cache_dir,
            take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses, daily_loss_limit,
            slippage, commission,
        } => {
            let paper_mode = mode.to_lowercase() != "live";

            if !paper_mode {
                println!("\n⚠️  LIVE TRADING MODE ⚠️");
                println!("This will execute REAL trades with REAL money.");
                println!("Type 'CONFIRM' to proceed:");

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.trim() != "CONFIRM" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let config = live_trader::LiveConfig {
                symbol,
                exchange,
                contracts,
                cache_dir,
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
                min_delta,
                max_lvn_ratio,
                level_tolerance,
                starting_balance,
                max_daily_losses,
                daily_loss_limit,
                point_value: 20.0, // NQ point value
                slippage,
                commission,
            };

            live_trader::run_live(config, paper_mode).await?;
        }
        Commands::IbTest => {
            ib_live::run_ib_demo()?;
        }
        Commands::IbLive {
            mode, host, port, client_id, contracts, cache_dir,
            take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses, daily_loss_limit,
        } => {
            let paper_mode = mode.to_lowercase() != "live";

            if !paper_mode {
                println!("\n⚠️  LIVE TRADING MODE ⚠️");
                println!("This will execute REAL trades with REAL money.");
                println!("Type 'CONFIRM' to proceed:");

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.trim() != "CONFIRM" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let config = live_trader::LiveConfig {
                symbol: "NQ".to_string(),
                exchange: "CME".to_string(),
                contracts,
                cache_dir,
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
                min_delta,
                max_lvn_ratio,
                level_tolerance,
                starting_balance,
                max_daily_losses,
                daily_loss_limit,
                point_value: 20.0,
                slippage: 0.0,
                commission: 0.0,
            };

            let ib_config = ib_live::IbConfig {
                host,
                port,
                client_id,
            };

            ib_live::run_ib_live(config, ib_config, paper_mode)?;
        }
        Commands::IbDataTest { symbol, duration } => {
            ib_live::run_ib_data_test(&symbol, duration)?;
        }
        Commands::IbFuturesTest { duration } => {
            ib_live::run_ib_futures_test(duration)?;
        }
        Commands::IbPolling {
            contracts, cache_dir, take_profit, trailing_stop, stop_buffer,
        } => {
            let config = live_trader::LiveConfig {
                symbol: "NQ".to_string(),
                exchange: "CME".to_string(),
                contracts,
                cache_dir,
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour: 9,
                start_minute: 30,
                end_hour: 16,
                end_minute: 0,
                min_delta: 60,
                max_lvn_ratio: 0.4,
                level_tolerance: 3.0,
                starting_balance: 30000.0,
                max_daily_losses: 3,
                daily_loss_limit: 100.0,
                point_value: 20.0,
                slippage: 0.0,
                commission: 0.0,
            };

            let ib_config = ib_live::IbConfig::default();
            ib_live::run_ib_polling_mode(config, ib_config)?;
        }
        Commands::DatabentoIbLive {
            mode, client_id, contracts, cache_dir, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses, daily_loss_limit,
        } => {
            let paper_mode = mode.to_lowercase() != "live";

            if !paper_mode {
                println!("\n⚠️  LIVE TRADING MODE ⚠️");
                println!("This will execute REAL trades with REAL money.");
                println!("Type 'CONFIRM' to proceed:");

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.trim() != "CONFIRM" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let api_key = std::env::var("DATABENTO_API_KEY")
                .context("DATABENTO_API_KEY not set")?;

            let config = live_trader::LiveConfig {
                symbol: "NQ".to_string(),
                exchange: "CME".to_string(),
                contracts,
                cache_dir,
                take_profit,
                trailing_stop,
                stop_buffer,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
                min_delta,
                max_lvn_ratio,
                level_tolerance,
                starting_balance,
                max_daily_losses,
                daily_loss_limit,
                point_value: 20.0,
                slippage: 0.0,
                commission: 0.0,
            };

            let ib_config = ib_live::IbConfig {
                client_id,
                ..ib_live::IbConfig::default()
            };
            databento_ib_live::run_databento_ib_live(api_key, config, ib_config, paper_mode).await?;
        }
    }

    Ok(())
}

async fn run_process(
    data_dir: PathBuf,
    output_dir: PathBuf,
    date: Option<String>,
    no_upload: bool,
) -> Result<()> {
    info!("=== PROCESS MODE ===");
    info!("Data directory: {:?}", data_dir);
    info!("Output directory: {:?}", output_dir);

    std::fs::create_dir_all(&output_dir)?;

    // Find all .zst files
    let zst_files = trades::find_zst_files(&data_dir, date.as_deref())?;
    info!("Found {} trade files to process", zst_files.len());

    if zst_files.is_empty() {
        info!("No files to process");
        return Ok(());
    }

    // Collect all data
    let mut all_bars = Vec::new();
    let mut all_daily_levels = Vec::new();
    let mut all_impulse_legs = Vec::new();
    let mut all_lvn_levels = Vec::new();

    for zst_path in &zst_files {
        info!("Processing: {:?}", zst_path);

        let trades = trades::parse_zst_trades(zst_path)?;
        info!("  Parsed {} trades", trades.len());

        if trades.is_empty() {
            continue;
        }

        let bars_1s = bars::aggregate_to_1s_bars(&trades);
        info!("  Created {} 1-second bars", bars_1s.len());

        let daily_levels = levels::compute_daily_levels(&bars_1s);
        info!("  Computed levels for {} trading days", daily_levels.len());

        let bars_1m = bars::aggregate_to_1m_bars(&bars_1s);
        info!("  Created {} 1-minute bars", bars_1m.len());

        let impulse_legs = impulse::detect_impulse_legs(&bars_1m, &daily_levels);
        info!("  Found {} valid impulse legs", impulse_legs.len());

        let lvn_levels = lvn::extract_lvns(&trades, &impulse_legs);
        info!("  Extracted {} LVN levels", lvn_levels.len());

        all_bars.extend(bars_1s);
        all_daily_levels.extend(daily_levels);
        all_impulse_legs.extend(impulse_legs);
        all_lvn_levels.extend(lvn_levels);
    }

    info!("Total: {} bars, {} daily levels, {} impulse legs, {} LVNs",
          all_bars.len(), all_daily_levels.len(),
          all_impulse_legs.len(), all_lvn_levels.len());

    // Write Parquet files
    info!("Writing Parquet files...");

    let bars_path = output_dir.join("replay_bars_1s.parquet");
    supabase::write_bars_parquet(&all_bars, &bars_path)?;
    info!("  Wrote {} bars to {:?}", all_bars.len(), bars_path);

    let levels_path = output_dir.join("daily_levels.parquet");
    supabase::write_levels_parquet(&all_daily_levels, &levels_path)?;
    info!("  Wrote {} daily levels to {:?}", all_daily_levels.len(), levels_path);

    let impulse_path = output_dir.join("impulse_legs.parquet");
    supabase::write_impulse_legs_parquet(&all_impulse_legs, &impulse_path)?;
    info!("  Wrote {} impulse legs to {:?}", all_impulse_legs.len(), impulse_path);

    let lvn_path = output_dir.join("lvn_levels.parquet");
    supabase::write_lvn_levels_parquet(&all_lvn_levels, &lvn_path)?;
    info!("  Wrote {} LVN levels to {:?}", all_lvn_levels.len(), lvn_path);

    // Generate signals by replaying through ProcessingState
    info!("Generating signals...");
    let zst_files_for_signals = trades::find_zst_files(&data_dir, date.as_deref())?;
    let mut all_trades_for_signals = Vec::new();
    for zst_path in &zst_files_for_signals {
        let trades = trades::parse_zst_trades(zst_path)?;
        all_trades_for_signals.extend(trades);
    }
    let signals = replay::replay_trades_for_signals(&all_trades_for_signals);
    info!("Generated {} signals", signals.len());

    // Write signals to Parquet
    let signals_path = output_dir.join("signals.parquet");
    replay::write_signals_parquet(&signals, &signals_path)?;
    info!("  Wrote {} signals to {:?}", signals.len(), signals_path);

    // Upload to Supabase
    if !no_upload {
        info!("Uploading to Supabase...");
        match supabase::SupabaseClient::from_env() {
            Ok(client) => {
                client.upload_bars(&all_bars).await?;
                client.upload_daily_levels(&all_daily_levels).await?;
                client.upload_impulse_legs(&all_impulse_legs).await?;
                client.upload_lvn_levels(&all_lvn_levels).await?;
                client.upload_signals(&signals).await?;
                info!("Upload complete!");
            }
            Err(e) => {
                info!("Skipping Supabase upload: {}", e);
            }
        }
    }

    info!("Process complete!");
    Ok(())
}

fn run_replay(
    data_dir: PathBuf,
    output_dir: PathBuf,
    date: Option<String>,
) -> Result<()> {
    info!("=== REPLAY MODE ===");
    info!("Replaying historical trades through production ProcessingState");
    info!("Data directory: {:?}", data_dir);

    std::fs::create_dir_all(&output_dir)?;

    // Parse trades
    let zst_files = trades::find_zst_files(&data_dir, date.as_deref())?;
    info!("Found {} trade files", zst_files.len());

    let mut all_trades = Vec::new();
    for zst_path in &zst_files {
        let trades = trades::parse_zst_trades(zst_path)?;
        info!("Parsed {} trades from {:?}", trades.len(), zst_path);
        all_trades.extend(trades);
    }

    info!("Total trades: {}", all_trades.len());

    // Replay through ProcessingState
    let signals = replay::replay_trades_for_signals(&all_trades);
    info!("Generated {} signals", signals.len());

    // Write signals to Parquet
    let signals_path = output_dir.join("signals.parquet");
    replay::write_signals_parquet(&signals, &signals_path)?;
    info!("Wrote signals to {:?}", signals_path);

    info!("Replay complete!");
    Ok(())
}

fn run_backtest(
    data_dir: PathBuf,
    output_dir: PathBuf,
    date: Option<String>,
    stop_loss: f64,
    take_profit: f64,
    max_hold: u64,
    rth_only: bool,
    min_confluence: u8,
    key_levels_only: bool,
) -> Result<()> {
    info!("=== BACKTEST MODE ===");
    info!("Running strategy backtest");
    info!("Data directory: {:?}", data_dir);

    std::fs::create_dir_all(&output_dir)?;

    // Parse trades and generate derived data
    let zst_files = trades::find_zst_files(&data_dir, date.as_deref())?;
    info!("Found {} trade files", zst_files.len());

    let mut all_trades = Vec::new();
    let mut all_bars = Vec::new();
    let mut all_daily_levels = Vec::new();

    for zst_path in &zst_files {
        let trades = trades::parse_zst_trades(zst_path)?;
        info!("Parsed {} trades from {:?}", trades.len(), zst_path);

        if !trades.is_empty() {
            let bars_1s = bars::aggregate_to_1s_bars(&trades);
            let daily_levels = levels::compute_daily_levels(&bars_1s);
            all_bars.extend(bars_1s);
            all_daily_levels.extend(daily_levels);
        }

        all_trades.extend(trades);
    }

    info!("Total: {} trades, {} bars, {} daily levels",
          all_trades.len(), all_bars.len(), all_daily_levels.len());

    // Replay through ProcessingState to get signals
    info!("Generating signals through replay...");
    let signals = replay::replay_trades_for_signals(&all_trades);
    info!("Generated {} signals", signals.len());

    // Configure backtest strategy
    let config = backtest::StrategyConfig {
        min_confluence_score: min_confluence,
        required_signals: vec![],
        stop_loss_points: stop_loss,
        take_profit_points: take_profit,
        max_hold_time_secs: max_hold,
        require_key_level: key_levels_only,
        min_strength: None,
        rth_only,
    };

    // Run backtest
    info!("Running backtest...");
    let backtester = backtest::Backtester::new(config, all_bars, all_daily_levels);
    let results = backtester.run(&signals);

    // Print results
    backtest::print_results(&results);

    // Write results to JSON
    let results_path = output_dir.join("backtest_results.json");
    let json = serde_json::to_string_pretty(&results)?;
    std::fs::write(&results_path, json)?;
    info!("Wrote results to {:?}", results_path);

    info!("Backtest complete!");
    Ok(())
}

async fn run_analyze(
    output_dir: PathBuf,
    stop_loss: f64,
    take_profit: f64,
    max_hold: u64,
    rth_only: bool,
    min_confluence: u8,
    key_levels_only: bool,
) -> Result<()> {
    info!("=== ANALYZE MODE ===");
    info!("Fetching data from Supabase for analysis");

    std::fs::create_dir_all(&output_dir)?;

    // Connect to Supabase
    let client = supabase::SupabaseClient::from_env()?;

    // Fetch data from Supabase
    let bars = client.fetch_bars().await?;
    let daily_levels = client.fetch_daily_levels().await?;
    let signals = client.fetch_signals().await?;

    info!("Loaded {} bars, {} daily levels, {} signals",
          bars.len(), daily_levels.len(), signals.len());

    // Debug: show sample signals
    for signal in signals.iter().take(3) {
        info!("  Signal: type={}, dir={}, price={:.2}, ts={}",
              signal.signal_type, signal.direction, signal.price, signal.timestamp);
    }

    // Debug: show sample bars
    for bar in bars.iter().take(3) {
        info!("  Bar: ts={}, close={:.2}",
              bar.timestamp.timestamp_millis(), bar.close);
    }

    if signals.is_empty() {
        info!("No signals found in Supabase. You need to run 'process' first to generate signals.");
        info!("Tip: Run 'pipeline process --data-dir <path>' to process trades and upload signals.");
        return Ok(());
    }

    // Configure backtest strategy
    let config = backtest::StrategyConfig {
        min_confluence_score: min_confluence,
        required_signals: vec![],
        stop_loss_points: stop_loss,
        take_profit_points: take_profit,
        max_hold_time_secs: max_hold,
        require_key_level: key_levels_only,
        min_strength: None,
        rth_only,
    };

    // Run backtest
    info!("Running backtest with SL={:.1}pts, TP={:.1}pts, MaxHold={}s...",
          stop_loss, take_profit, max_hold);
    let backtester = backtest::Backtester::new(config, bars, daily_levels);
    let results = backtester.run(&signals);

    // Print results
    backtest::print_results(&results);

    // Write results to JSON
    let results_path = output_dir.join("analyze_results.json");
    let json = serde_json::to_string_pretty(&results)?;
    std::fs::write(&results_path, json)?;
    info!("Wrote results to {:?}", results_path);

    info!("Analysis complete!");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_three_element(
    data_dir: PathBuf,
    output_dir: PathBuf,
    cache_dir: Option<PathBuf>,
    date: Option<String>,
    lookback: usize,
    level_tolerance: f64,
    mr_stop_loss: f64,
    mr_take_profit: f64,
    tc_stop_loss: f64,
    tc_take_profit: f64,
    tc_trailing_stop: f64,
    delta_threshold: i64,
    delta_lookback: usize,
    imbalance_ratio: f64,
    rth_only: bool,
    use_signals: bool,
    global_cooldown: usize,
    level_cooldown: usize,
    ms_delta_threshold: i64,
) -> Result<()> {
    info!("=== THREE-ELEMENT BACKTEST ===");
    info!("Market State + Location + Aggression");

    std::fs::create_dir_all(&output_dir)?;

    // Try to load from cache first
    let (all_bars_1m, all_lvn_levels, signals) = if let Some(ref cache_path) = cache_dir {
        info!("Loading from cache: {:?}", cache_path);
        let start = std::time::Instant::now();

        let days = precompute::load_all_cached(cache_path, date.as_deref())?;

        if days.is_empty() {
            anyhow::bail!("No cached data found. Run 'precompute' first.");
        }

        let mut all_bars = Vec::new();
        let mut all_lvns = Vec::new();
        let mut all_signals = Vec::new();

        for day in days {
            all_bars.extend(day.bars_1s);
            all_lvns.extend(day.lvn_levels);
            all_signals.extend(day.signals);
        }

        info!("Loaded from cache in {:.2}s: {} bars, {} LVNs, {} signals",
              start.elapsed().as_secs_f64(),
              all_bars.len(), all_lvns.len(), all_signals.len());

        (all_bars, all_lvns, all_signals)
    } else {
        info!("Data directory: {:?}", data_dir);

        // Parse trades and compute derived data
        let zst_files = trades::find_zst_files(&data_dir, date.as_deref())?;
        info!("Found {} trade files", zst_files.len());

        let mut all_trades = Vec::new();
        let mut all_bars_1m = Vec::new();
        let mut all_lvn_levels = Vec::new();

        for zst_path in &zst_files {
            let trades = trades::parse_zst_trades(zst_path)?;
            info!("Parsed {} trades from {:?}", trades.len(), zst_path);

            if !trades.is_empty() {
                // Create 1-second bars for precise trade simulation
                let bars_1s = bars::aggregate_to_1s_bars(&trades);
                info!("  Created {} 1-second bars", bars_1s.len());

                // Also create 1-minute bars for impulse leg detection
                let bars_1m = bars::aggregate_to_1m_bars(&bars_1s);

                // Compute daily levels for impulse leg detection
                let daily_levels = levels::compute_daily_levels(&bars_1s);

                // Detect impulse legs (uses 1-min bars)
                let impulse_legs = impulse::detect_impulse_legs(&bars_1m, &daily_levels);
                info!("  Found {} impulse legs", impulse_legs.len());

                // Extract LVNs from impulse legs
                let lvn_levels = lvn::extract_lvns(&trades, &impulse_legs);
                info!("  Extracted {} LVN levels", lvn_levels.len());

                // Use 1-second bars for backtesting (precise simulation)
                all_bars_1m.extend(bars_1s);
                all_lvn_levels.extend(lvn_levels);
            }

            all_trades.extend(trades);
        }

        info!("Total: {} trades, {} 1-second bars, {} LVN levels",
              all_trades.len(), all_bars_1m.len(), all_lvn_levels.len());

        // Generate signals from replay if enabled
        let signals = if use_signals {
            info!("Generating signals through replay...");
            let signals = replay::replay_trades_for_signals(&all_trades);
            info!("Generated {} signals", signals.len());
            signals
        } else {
            info!("Using aggression detection from bars only (no captured signals)");
            Vec::new()
        };

        (all_bars_1m, all_lvn_levels, signals)
    };

    // Configure the three-element backtester
    let config = three_element_backtest::ThreeElementConfig {
        market_state: market_state::MarketStateConfig {
            lookback_bars: lookback,
            delta_accumulation_threshold: ms_delta_threshold,
            ..Default::default()
        },
        aggression: three_element_backtest::AggressionConfig {
            lookback: delta_lookback,
            delta_momentum_threshold: delta_threshold,
            imbalance_ratio_threshold: imbalance_ratio,
            volume_spike_mult: 5.0, // 5x average = significant spike
            min_volume: 50,         // Minimum 50 contracts (filter noise)
            use_captured_signals: use_signals,
        },
        level_tolerance,
        mr_stop_loss,
        mr_take_profit,
        mr_max_hold_bars: 180,  // 3 minutes in 1-second bars
        tc_stop_loss,
        tc_take_profit,
        tc_max_hold_bars: 600,  // 10 minutes in 1-second bars
        tc_trailing_stop,
        rth_only,
        global_cooldown,
        level_cooldown,
    };

    info!("Config:");
    info!("  Market State lookback: {} bars", lookback);
    info!("  Level tolerance: {} pts", level_tolerance);
    info!("  Mean Reversion: SL={} TP={}", mr_stop_loss, mr_take_profit);
    info!("  Trend Continuation: SL={} TP={} Trail={}", tc_stop_loss, tc_take_profit, tc_trailing_stop);
    info!("  Delta threshold: {}, lookback: {} bars", delta_threshold, delta_lookback);
    info!("  Imbalance ratio: {}:1", imbalance_ratio);
    info!("  Cooldowns: global={} sec, per-level={} sec", global_cooldown, level_cooldown);
    info!("  RTH only: {}", rth_only);

    // Run the backtest
    let backtester = three_element_backtest::ThreeElementBacktester::new(
        all_bars_1m,
        signals,
        all_lvn_levels,
        config,
    );

    let results = backtester.run();

    // Print results
    three_element_backtest::print_results(&results);

    // Write results to JSON
    let results_path = output_dir.join("three_element_results.json");
    let json = serde_json::to_string_pretty(&results)?;
    std::fs::write(&results_path, json)?;
    info!("Wrote results to {:?}", results_path);

    info!("Three-element backtest complete!");
    Ok(())
}

fn run_precompute(
    data_dir: PathBuf,
    cache_dir: PathBuf,
    date: Option<String>,
) -> Result<()> {
    info!("=== PRECOMPUTE SIGNALS ===");
    info!("Data directory: {:?}", data_dir);
    info!("Cache directory: {:?}", cache_dir);

    std::fs::create_dir_all(&cache_dir)?;

    // Find all .zst files
    let zst_files = trades::find_zst_files(&data_dir, date.as_deref())?;
    info!("Found {} trade files to process", zst_files.len());

    if zst_files.is_empty() {
        info!("No files to process");
        return Ok(());
    }

    // Check which dates already have cached data
    let cached_dates = precompute::get_cached_dates(&cache_dir)?;
    info!("Found {} cached dates", cached_dates.len());

    // Filter out already cached files
    let files_to_process: Vec<_> = zst_files
        .into_iter()
        .filter(|path| {
            if let Some(date) = precompute::extract_date_from_path(path) {
                !cached_dates.contains(&date)
            } else {
                true
            }
        })
        .collect();

    info!("{} files need processing", files_to_process.len());

    if files_to_process.is_empty() {
        info!("All data already cached!");
        return Ok(());
    }

    // Process in parallel
    info!("Processing {} days in parallel...", files_to_process.len());
    let start = std::time::Instant::now();

    let results = precompute::process_days_parallel(&files_to_process);

    // Save successful results
    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok(data) => {
                if let Err(e) = precompute::save_day_cache(&data, &cache_dir) {
                    info!("Failed to save cache for {}: {}", data.date, e);
                    error_count += 1;
                } else {
                    success_count += 1;
                }
            }
            Err(e) => {
                info!("Failed to process: {}", e);
                error_count += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    info!(
        "Precompute complete: {} succeeded, {} failed in {:.1}s",
        success_count,
        error_count,
        elapsed.as_secs_f64()
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_lvn_retest(
    cache_dir: PathBuf,
    date: Option<String>,
    level_tolerance: f64,
    retest_distance: f64,
    min_delta: i64,
    max_range: f64,
    stop_loss: f64,
    take_profit: f64,
    trailing_stop: f64,
    rth_only: bool,
    max_lvn_ratio: f64,
    same_day_only: bool,
    stop_buffer: f64,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
    reverse_exit: bool,
    reverse_delta: i64,
) -> Result<()> {
    info!("=== LVN RETEST STRATEGY ===");
    info!("Loading from cache: {:?}", cache_dir);

    let start = std::time::Instant::now();

    // Load cached data
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    // Combine all days
    let mut all_bars = Vec::new();
    let mut all_lvns = Vec::new();

    for day in days {
        all_bars.extend(day.bars_1s);
        all_lvns.extend(day.lvn_levels);
    }

    info!(
        "Loaded in {:.2}s: {} bars, {} LVN levels",
        start.elapsed().as_secs_f64(),
        all_bars.len(),
        all_lvns.len()
    );

    // Configure the backtester
    let config = lvn_retest::LvnRetestConfig {
        level_tolerance,
        retest_distance,
        min_delta_for_absorption: min_delta,
        max_range_for_absorption: max_range,
        stop_loss,
        take_profit,
        trailing_stop,
        rth_only,
        max_lvn_volume_ratio: max_lvn_ratio,
        same_day_only,
        structure_stop_buffer: stop_buffer,
        trade_start_hour: start_hour,
        trade_start_minute: start_minute,
        trade_end_hour: end_hour,
        trade_end_minute: end_minute,
        // exit_on_reverse_aggression: reverse_exit,
        // reverse_aggression_delta: reverse_delta,
        ..Default::default()
    };

    // Suppress unused variable warnings
    let _ = (reverse_exit, reverse_delta);

    info!("Config:");
    info!("  Level tolerance: {} pts", level_tolerance);
    info!("  Retest distance: {} pts", retest_distance);
    info!("  Absorption: delta >= {}, range <= {} pts", min_delta, max_range);
    info!("  SL: {} pts, TP: {} pts, Trail: {} pts", stop_loss, take_profit, trailing_stop);
    info!("  Stop buffer: {} pts beyond LVN", stop_buffer);
    info!("  Trading hours: {:02}:{:02} - {:02}:{:02} ET", start_hour, start_minute, end_hour, end_minute);
    // if reverse_exit {
    //     info!("  REVERSE EXIT MODE: Exit on delta {} against position", reverse_delta);
    // }
    info!("  Max LVN ratio: {} (quality filter)", max_lvn_ratio);
    info!("  Same-day only: {}", same_day_only);

    // Run backtest
    let backtester = lvn_retest::LvnRetestBacktester::new(all_bars, all_lvns, config);
    let results = backtester.run();

    // Print results
    lvn_retest::print_results(&results);

    info!("LVN Retest backtest complete!");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_paper_trade(
    cache_dir: PathBuf,
    state_file: PathBuf,
    log_file: PathBuf,
    date: Option<String>,
    level_tolerance: f64,
    min_delta: i64,
    max_range: f64,
    take_profit: f64,
    trailing_stop: f64,
    stop_buffer: f64,
    max_lvn_ratio: f64,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
    speed: u32,
    live_status: bool,
) -> Result<()> {
    info!("=== PAPER TRADING VALIDATION ===");
    info!("Loading from cache: {:?}", cache_dir);

    let start = std::time::Instant::now();

    // Load cached data
    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    if days.is_empty() {
        anyhow::bail!("No cached data found. Run 'precompute' first.");
    }

    info!(
        "Loaded in {:.2}s: {} days of data",
        start.elapsed().as_secs_f64(),
        days.len()
    );

    // Configure paper trading
    let config = paper_trading::PaperConfig {
        level_tolerance,
        retest_distance: 8.0,
        min_delta,
        max_range,
        take_profit,
        trailing_stop,
        stop_buffer,
        max_lvn_ratio,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
    };

    info!("Paper Trading Config:");
    info!("  Trading hours: {:02}:{:02} - {:02}:{:02} ET", start_hour, start_minute, end_hour, end_minute);
    info!("  Level tolerance: {} pts", level_tolerance);
    info!("  Signal: delta >= {}, range <= {} pts", min_delta, max_range);
    info!("  TP: {} pts, Trail: {} pts, Stop buffer: {} pts", take_profit, trailing_stop, stop_buffer);
    info!("  Max LVN ratio: {}", max_lvn_ratio);
    info!("  Speed: {}x (0 = max speed)", speed);

    // Initialize paper trading state
    let mut state = paper_trading::PaperTradingState::new(config);

    // Process each day
    let mut total_bars = 0;
    let mut total_signals = 0;

    for day in days {
        let day_date = &day.date;
        info!("Processing day: {}", day_date);

        // Add LVN levels for this day
        state.add_lvn_levels(&day.lvn_levels);
        info!("  Added {} LVN levels", day.lvn_levels.len());

        // Process bars
        for bar in &day.bars_1s {
            total_bars += 1;

            if let Some(signal) = state.process_bar(bar) {
                total_signals += 1;

                // Print alert
                paper_trading::print_signal_alert(&signal);

                // Log to file
                if let Err(e) = paper_trading::log_signal(&signal, &log_file) {
                    info!("Failed to log signal: {}", e);
                }
            }

            // Log closed trades
            let trades = state.get_closed_trades();
            if let Some(last_trade) = trades.last() {
                if last_trade.exit_time.is_some() {
                    if let Err(e) = paper_trading::log_trade(last_trade, &log_file) {
                        info!("Failed to log trade: {}", e);
                    }
                }
            }

            // Show live status updates periodically
            if live_status && total_bars % 3600 == 0 {
                state.print_status();
            }

            // Speed control
            if speed > 0 {
                std::thread::sleep(std::time::Duration::from_millis(1000 / speed as u64));
            }
        }

        // Clear levels at end of day (for same-session freshness)
        state.clear_levels();
    }

    // Final status
    state.print_status();

    // Print summary
    let stats = state.calculate_stats();
    println!("\n═══════════════════════════════════════════════════════════");
    println!("              PAPER TRADING SUMMARY                         ");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Bars Processed:    {}", total_bars);
    println!("Signals Generated: {}", total_signals);
    println!("Trades Executed:   {}", stats.total_trades);
    println!();
    println!("Wins:              {} ({:.1}%)", stats.wins, stats.win_rate);
    println!("Losses:            {}", stats.losses);
    println!("Total P&L:         {:+.2} pts (${:.2})", stats.total_pnl, stats.total_pnl * 20.0);
    println!();
    println!("Avg Win:           {:+.2} pts", stats.avg_win);
    println!("Avg Loss:          {:+.2} pts", stats.avg_loss);
    println!("R:R Ratio:         {:.2}:1", stats.rr_ratio);
    println!("Profit Factor:     {:.2}", stats.profit_factor);

    println!("\n═══════════════════════════════════════════════════════════\n");

    // Save state
    if let Err(e) = state.save(&state_file) {
        info!("Failed to save state: {}", e);
    } else {
        info!("State saved to {:?}", state_file);
    }

    info!("Signals logged to {:?}", log_file);
    info!("Paper trading validation complete!");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_trade(
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
    speed: u32,
) -> Result<()> {
    live_trading::run_trading(
        mode,
        contracts,
        daily_loss_limit,
        take_profit,
        trailing_stop,
        stop_buffer,
        cache_dir,
        date,
        start_hour,
        start_minute,
        end_hour,
        end_minute,
        min_delta,
        max_lvn_ratio,
        level_tolerance,
        starting_balance,
        speed,
    ).await
}
