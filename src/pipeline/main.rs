mod bars;
mod backtest;
mod databento_ib_live;
mod fetch_date;
mod ib_execution;
mod impulse;
mod levels;
mod lvn;
mod trader;
mod lvn_retest;
mod market_state;
mod monte_carlo;
mod precompute;
mod replay;
mod replay_trading;
mod smart_lvn;
mod state_machine;
mod supabase;
mod sweep;
mod three_element_backtest;
mod trades;

use anyhow::{Context, Result};
use chrono::Datelike;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn};
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
        #[arg(long, default_value = "0.5")]
        slippage: f64,

        /// Commission per round-trip in dollars
        #[arg(long, default_value = "4.0")]
        commission: f64,

        /// Maximum win cap in points (0 = disabled)
        #[arg(long, default_value = "30.0")]
        max_win_cap: f64,

        /// Volatility slippage factor (extra_slippage = bar_range * factor)
        #[arg(long, default_value = "0.1")]
        volatility_slippage_factor: f64,

        /// Outlier threshold for statistics (trades above excluded, 0 = disabled)
        #[arg(long, default_value = "50.0")]
        outlier_threshold: f64,
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

        /// Maximum impulse bars (1s bars, default 200 = 3.3 min)
        #[arg(long, default_value = "200")]
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

        /// Maximum win cap (points) to exclude outliers
        #[arg(long, default_value = "20.0")]
        max_win_cap: f64,

        /// Volatility slippage factor
        #[arg(long, default_value = "0.1")]
        volatility_slippage_factor: f64,

        /// Outlier threshold (points)
        #[arg(long, default_value = "50.0")]
        outlier_threshold: f64,
    },

    /// Replay test with PRIOR DAY FULL PROFILE LVNs (no real-time impulse detection)
    ReplayProfile {
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

        /// Minimum delta for absorption signal
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

        /// Maximum win cap (points) to exclude outliers
        #[arg(long, default_value = "20.0")]
        max_win_cap: f64,

        /// Volatility slippage factor
        #[arg(long, default_value = "0.1")]
        volatility_slippage_factor: f64,

        /// Outlier threshold (points)
        #[arg(long, default_value = "50.0")]
        outlier_threshold: f64,
    },

    /// Smart LVN backtest (discretionary trader's process)
    SmartLvn {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Maximum trades per day
        #[arg(long, default_value = "5")]
        max_trades_per_day: usize,

        /// Minimum delta confirmation at level
        #[arg(long, default_value = "30")]
        min_delta: i64,

        /// Minimum impulse size in points
        #[arg(long, default_value = "25")]
        min_impulse_size: f64,

        /// Maximum impulse bars (1s)
        #[arg(long, default_value = "120")]
        max_impulse_bars: usize,

        /// Level tolerance in points
        #[arg(long, default_value = "2.0")]
        level_tolerance: f64,

        /// Trailing stop in points
        #[arg(long, default_value = "4.0")]
        trailing_stop: f64,

        /// Take profit in points (0 = trailing only)
        #[arg(long, default_value = "20.0")]
        take_profit: f64,

        /// Stop buffer beyond LVN
        #[arg(long, default_value = "2.0")]
        stop_buffer: f64,

        /// Trading start hour (ET)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading end hour (ET)
        #[arg(long, default_value = "15")]
        end_hour: u32,
    },

    /// Real LVN backtest - uses actual volume profile LVNs (not Fibonacci)
    RealLvn {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Minimum delta confirmation at level
        #[arg(long, default_value = "50")]
        min_delta: i64,

        /// Level tolerance in points
        #[arg(long, default_value = "2.0")]
        level_tolerance: f64,

        /// Trailing stop in points
        #[arg(long, default_value = "6.0")]
        trailing_stop: f64,

        /// Take profit in points (0 = trailing only)
        #[arg(long, default_value = "0")]
        take_profit: f64,

        /// Stop buffer beyond LVN
        #[arg(long, default_value = "2.0")]
        stop_buffer: f64,

        /// Maximum trades per day
        #[arg(long, default_value = "5")]
        max_trades_per_day: usize,

        /// Trading start hour (ET)
        #[arg(long, default_value = "9")]
        start_hour: u32,

        /// Trading end hour (ET)
        #[arg(long, default_value = "15")]
        end_hour: u32,
    },

    /// Smart LVN Exit Strategy Sweep - find optimal trailing stop / take profit
    SmartLvnExitSweep {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Fixed: Minimum delta confirmation at level
        #[arg(long, default_value = "150")]
        min_delta: i64,

        /// Fixed: Minimum impulse size in points
        #[arg(long, default_value = "35")]
        min_impulse_size: f64,

        /// Trailing stop values to sweep (comma-separated)
        #[arg(long, default_value = "4,6,8,10,12,15")]
        trailing_stops: String,

        /// Take profit values to sweep (comma-separated, 0 = trailing only)
        #[arg(long, default_value = "0,15,20,25,30,40")]
        take_profits: String,
    },

    /// Multi-dimensional parameter sweep (delta × trailing × impulse × time)
    MultiSweep {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Process only a specific date (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: Option<String>,

        /// Start date for filtering (YYYYMMDD format, inclusive)
        #[arg(long)]
        start_date: Option<String>,

        /// End date for filtering (YYYYMMDD format, inclusive)
        #[arg(long)]
        end_date: Option<String>,

        /// Delta values to sweep (comma-separated)
        #[arg(long, default_value = "100,150,200,250,300")]
        deltas: String,

        /// Trailing stop values to sweep (comma-separated)
        #[arg(long, default_value = "1.5,2,2.5,3,4")]
        trailing_stops: String,

        /// Impulse size values to sweep (comma-separated)
        #[arg(long, default_value = "25,35,50,75")]
        impulse_sizes: String,

        /// Time windows to test (all,morning,midday,afternoon,open_hour)
        #[arg(long, default_value = "all,morning,midday,afternoon,open_hour")]
        time_windows: String,

        /// Top N results to display
        #[arg(long, default_value = "30")]
        top_n: usize,
    },

    /// Analyze delta distribution to calibrate optimal threshold
    AnalyzeDelta {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Minimum impulse size in points
        #[arg(long, default_value = "35")]
        min_impulse_size: f64,
    },

    /// Fetch and precompute data for a specific date from Databento
    FetchDate {
        /// Date to fetch (YYYYMMDD format)
        #[arg(short = 'D', long)]
        date: String,

        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Symbol to fetch (e.g., NQH6 for March 2026)
        #[arg(short, long, default_value = "NQH6")]
        symbol: String,
    },

    /// Batch fetch and precompute data for a date range from Databento
    BatchFetch {
        /// Start date (YYYYMMDD format)
        #[arg(long)]
        start: String,

        /// End date (YYYYMMDD format)
        #[arg(long)]
        end: String,

        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_es_2025")]
        cache_dir: PathBuf,

        /// Base symbol (ES or NQ) - will auto-roll contracts
        #[arg(short, long, default_value = "ES")]
        symbol: String,
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

    /// Live trading with Databento data + IB execution (production mode)
    DatabentoIbLive {
        /// Trading mode: "observe" (no execution, just track), "paper", or "live"
        #[arg(long, default_value = "observe")]
        mode: String,

        /// Contract symbol (e.g., NQH6 for March 2026, NQM6 for June 2026)
        #[arg(long, default_value = "NQH6")]
        contract_symbol: String,

        /// Output file for trade log (CSV format)
        #[arg(long, default_value = "trades.csv")]
        trade_log: PathBuf,

        /// IB Client ID (must be unique per connection, ignored in observe mode)
        #[arg(long, default_value = "2")]
        client_id: i32,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points (500 = effectively disabled, rely on trailing stop)
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

        // --- State Machine Parameters ---

        /// Breakout threshold in points beyond level
        #[arg(long, default_value = "2.0")]
        breakout_threshold: f64,

        /// Minimum impulse size in points
        #[arg(long, default_value = "15")]
        min_impulse_size: f64,

        /// Minimum impulse score (out of 5 criteria)
        #[arg(long, default_value = "3")]
        min_impulse_score: u8,

        /// Maximum bars for impulse profiling
        #[arg(long, default_value = "200")]
        max_impulse_bars: usize,

        /// Maximum bars to hunt for retest
        #[arg(long, default_value = "1800")]
        max_hunting_bars: usize,

        /// Maximum retrace ratio before invalidation
        #[arg(long, default_value = "0.7")]
        max_retrace_ratio: f64,
    },

    /// Live trading with Databento data + TopstepX execution (prop firm trading)
    TopstepLive {
        /// Contract symbol (e.g., NQH6 for March 2026)
        #[arg(long, default_value = "NQH6")]
        contract_symbol: String,

        /// Output file for trade log (CSV format)
        #[arg(long, default_value = "trades.csv")]
        trade_log: PathBuf,

        /// Number of contracts
        #[arg(long, default_value = "1")]
        contracts: i32,

        /// Cache directory for LVN levels
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Take profit in points
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

        /// Minimum delta for signal
        #[arg(long, default_value = "150")]
        min_delta: i64,

        /// Starting balance for tracking
        #[arg(long, default_value = "50000")]
        starting_balance: f64,

        /// Max losing trades per day
        #[arg(long, default_value = "3")]
        max_daily_losses: i32,

        /// Daily loss limit in points
        #[arg(long, default_value = "100")]
        daily_loss_limit: f64,

        // State Machine Parameters
        /// Breakout threshold in points
        #[arg(long, default_value = "2.0")]
        breakout_threshold: f64,

        /// Minimum impulse size in points
        #[arg(long, default_value = "25")]
        min_impulse_size: f64,

        /// Minimum impulse score
        #[arg(long, default_value = "3")]
        min_impulse_score: u8,

        /// Maximum bars for impulse profiling
        #[arg(long, default_value = "200")]
        max_impulse_bars: usize,

        /// Maximum bars to hunt for retest
        #[arg(long, default_value = "1800")]
        max_hunting_bars: usize,

        /// Maximum retrace ratio
        #[arg(long, default_value = "0.7")]
        max_retrace_ratio: f64,
    },

    /// Test Databento live data feed (no trading, just displays data)
    DatabentoTest {
        /// Symbol to subscribe to (e.g., NQH6 for March 2026 NQ)
        #[arg(long, default_value = "NQH6")]
        symbol: String,

        /// Duration in seconds (0 = run indefinitely)
        #[arg(long, default_value = "60")]
        duration: u64,
    },

    /// Parameter sweep - test many configurations in parallel
    Sweep {
        /// Cache directory for precomputed data
        #[arg(short, long, default_value = "cache_2025")]
        cache_dir: PathBuf,

        /// Output CSV file for results
        #[arg(short, long, default_value = "sweep_results.csv")]
        output: PathBuf,

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

        /// Minimum delta values (comma-separated)
        #[arg(long, default_value = "15,20,25,30,35,40")]
        min_delta: String,

        /// Trailing stop values (comma-separated)
        #[arg(long, default_value = "5,6,7,8")]
        trailing_stop: String,

        /// Take profit values (comma-separated)
        #[arg(long, default_value = "25,30,35")]
        take_profit: String,

        /// Stop buffer values (comma-separated)
        #[arg(long, default_value = "2,3")]
        stop_buffer: String,

        /// Max hunting bars values (comma-separated)
        #[arg(long, default_value = "600,900")]
        max_hunting_bars: String,

        /// Min impulse score values (comma-separated)
        #[arg(long, default_value = "4")]
        min_impulse_score: String,
    },
}

/// Get the front-month contract symbol for a given date
/// Futures roll on the 3rd Friday of the expiration month, but we use a simplified rule:
/// - Use the next quarterly contract (H=Mar, M=Jun, U=Sep, Z=Dec)
/// - Roll 2 weeks before expiration
fn get_front_month_contract(base_symbol: &str, date: chrono::NaiveDate) -> String {
    let year = date.year();
    let month = date.month();

    // Determine the front month contract
    // Quarters: Mar(H), Jun(M), Sep(U), Dec(Z)
    // Roll ~2 weeks before expiration (mid-month before quarter end)
    let (contract_month, contract_year) = match month {
        1 | 2 => ('H', year),           // Jan-Feb -> March
        3 => {
            if date.day() < 10 { ('H', year) } else { ('M', year) }  // Early Mar -> H, Late Mar -> M
        }
        4 | 5 => ('M', year),           // Apr-May -> June
        6 => {
            if date.day() < 10 { ('M', year) } else { ('U', year) }
        }
        7 | 8 => ('U', year),           // Jul-Aug -> September
        9 => {
            if date.day() < 10 { ('U', year) } else { ('Z', year) }
        }
        10 | 11 => ('Z', year),         // Oct-Nov -> December
        12 => {
            if date.day() < 10 { ('Z', year) } else { ('H', year + 1) }
        }
        _ => ('H', year),
    };

    // Year code is last digit (e.g., 2025 -> 5, 2026 -> 6)
    let year_code = contract_year % 10;

    format!("{}{}{}", base_symbol, contract_month, year_code)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args = Args::parse();

    // Set up logging with filter to reduce noise from hitthebid processing
    // The processing module logs every bubble creation at INFO level which is too verbose
    let is_sweep = matches!(args.command, Commands::Sweep { .. });
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else if is_sweep {
        // Suppress INFO logging during sweep for performance
        EnvFilter::new("pipeline=warn,hitthebid=warn")
    } else {
        // Only show warnings from hitthebid, INFO from pipeline
        EnvFilter::new("pipeline=info,hitthebid=warn")
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
        Commands::MonteCarlo { simulations: _ } => {
            monte_carlo::run_monte_carlo();
        }
        Commands::MonteCarloEtf { simulations: _ } => {
            monte_carlo::run_etf_monte_carlo();
        }
        Commands::ReplayTest {
            cache_dir, date,
            contracts, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses,
            slippage, commission,
            max_win_cap, volatility_slippage_factor, outlier_threshold,
        } => {
            // Use same LiveConfig as live trading - validates exact same code path
            let config = trader::LiveConfig {
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
                max_win_cap,
                volatility_slippage_factor,
                outlier_threshold,
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
            max_win_cap, volatility_slippage_factor, outlier_threshold,
        } => {
            let config = trader::LiveConfig {
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
                slippage: 0.5, // Default realistic slippage
                commission: 4.0, // Default commission
                max_win_cap,
                volatility_slippage_factor,
                outlier_threshold,
            };

            let sm_config = state_machine::StateMachineConfig {
                breakout_threshold,
                max_impulse_bars,
                min_impulse_size,
                max_hunting_bars,
                min_impulse_score,
                max_retrace_ratio,
                min_bars_before_switch: 60, // 1 minute before switching to new breakout
            };

            replay_trading::run_replay_realtime(cache_dir, date, config, sm_config).await?;
        }
        Commands::ReplayProfile {
            cache_dir, date,
            contracts, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance,
            max_win_cap, volatility_slippage_factor, outlier_threshold,
        } => {
            let config = trader::LiveConfig {
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
                max_daily_losses: 0,
                daily_loss_limit: 1000.0,
                point_value: 20.0,
                slippage: 0.5,
                commission: 4.0,
                max_win_cap,
                volatility_slippage_factor,
                outlier_threshold,
            };

            replay_trading::run_replay_prior_day_profile(cache_dir, date, config).await?;
        }
        Commands::SmartLvn {
            cache_dir, date,
            max_trades_per_day, min_delta, min_impulse_size,
            max_impulse_bars, level_tolerance,
            trailing_stop, take_profit, stop_buffer,
            start_hour, end_hour,
        } => {
            info!("=== SMART LVN BACKTEST ===");
            info!("Implementing discretionary trader's process:");
            info!("  - Valid impulse = Balanced → Imbalanced transition");
            info!("  - First touch only (trapped traders)");
            info!("  - Delta confirmation at level (min {})", min_delta);
            info!("  - Max {} trades/day", max_trades_per_day);

            // Load cached data
            let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

            if days.is_empty() {
                anyhow::bail!("No cached data found. Run 'precompute' first.");
            }

            info!("Loaded {} days of data", days.len());

            let config = smart_lvn::SmartLvnConfig {
                max_trades_per_day,
                min_delta_confirmation: min_delta,
                min_impulse_size,
                max_impulse_bars,
                level_tolerance,
                trailing_stop,
                take_profit,
                stop_buffer,
                start_hour,
                end_hour,
            };

            let backtest = smart_lvn::SmartLvnBacktest::new(config);
            let result = backtest.run(&days);

            println!("{}", result);
        }
        Commands::RealLvn {
            cache_dir, date,
            min_delta, level_tolerance,
            trailing_stop, take_profit, stop_buffer,
            max_trades_per_day, start_hour, end_hour,
        } => {
            info!("=== REAL LVN BACKTEST ===");
            info!("Using actual volume profile LVNs (not Fibonacci proxy)");
            info!("  - Delta confirmation: {}", min_delta);
            info!("  - Max {} trades/day", max_trades_per_day);
            info!("  - Trailing stop: {} pts, TP: {} pts", trailing_stop, take_profit);

            let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

            if days.is_empty() {
                anyhow::bail!("No cached data found. Run 'precompute' first.");
            }

            let total_lvns: usize = days.iter().map(|d| d.lvn_levels.len()).sum();
            info!("Loaded {} days, {} real LVNs", days.len(), total_lvns);

            let config = smart_lvn::SmartLvnConfig {
                max_trades_per_day,
                min_delta_confirmation: min_delta,
                min_impulse_size: 0.0, // Not used for real LVN
                max_impulse_bars: 0,   // Not used for real LVN
                level_tolerance,
                trailing_stop,
                take_profit,
                stop_buffer,
                start_hour,
                end_hour,
            };

            let backtest = smart_lvn::RealLvnBacktest::new(config);
            let result = backtest.run(&days);

            println!("{}", result);
        }
        Commands::SmartLvnExitSweep {
            cache_dir, date,
            min_delta, min_impulse_size,
            trailing_stops, take_profits,
        } => {
            info!("=== SMART LVN EXIT STRATEGY SWEEP ===");
            info!("Fixed entry: delta={}, impulse_size={}", min_delta, min_impulse_size);

            // Parse sweep values
            let trailing_stop_values: Vec<f64> = trailing_stops
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let take_profit_values: Vec<f64> = take_profits
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            info!("Trailing stops to test: {:?}", trailing_stop_values);
            info!("Take profits to test: {:?}", take_profit_values);
            info!("Total combinations: {}", trailing_stop_values.len() * take_profit_values.len());

            // Load cached data
            let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

            if days.is_empty() {
                anyhow::bail!("No cached data found. Run 'precompute' first.");
            }

            info!("Loaded {} days of data", days.len());

            // Run the sweep
            let results = smart_lvn::run_exit_sweep(
                &days,
                min_delta,
                min_impulse_size,
                &trailing_stop_values,
                &take_profit_values,
            );

            // Print results
            smart_lvn::print_exit_sweep_results(&results, min_delta, min_impulse_size);
        }
        Commands::MultiSweep {
            cache_dir, date, start_date, end_date,
            deltas, trailing_stops, impulse_sizes, time_windows,
            top_n,
        } => {
            info!("=== MULTI-DIMENSIONAL PARAMETER SWEEP ===");
            info!("Testing all combinations of delta × trailing × impulse × time");

            // Parse sweep values
            let delta_values: Vec<i64> = deltas
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let trailing_stop_values: Vec<f64> = trailing_stops
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let impulse_size_values: Vec<f64> = impulse_sizes
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            // Parse time windows
            let time_window_values: Vec<smart_lvn::TimeWindow> = time_windows
                .split(',')
                .filter_map(|s| match s.trim() {
                    "all" => Some(smart_lvn::TimeWindow::ALL_DAY),
                    "morning" => Some(smart_lvn::TimeWindow::MORNING),
                    "midday" => Some(smart_lvn::TimeWindow::MIDDAY),
                    "afternoon" => Some(smart_lvn::TimeWindow::AFTERNOON),
                    "open_hour" => Some(smart_lvn::TimeWindow::OPEN_HOUR),
                    _ => None,
                })
                .collect();

            let total = delta_values.len() * trailing_stop_values.len() *
                       impulse_size_values.len() * time_window_values.len();
            info!("Deltas: {:?}", delta_values);
            info!("Trailing stops: {:?}", trailing_stop_values);
            info!("Impulse sizes: {:?}", impulse_size_values);
            info!("Time windows: {:?}", time_windows);
            info!("Total combinations: {}", total);

            // Load cached data
            let mut days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

            // Filter by date range if specified
            if let Some(ref start) = start_date {
                days.retain(|d| d.date.as_str() >= start.as_str());
                info!("Filtering from {}", start);
            }
            if let Some(ref end) = end_date {
                days.retain(|d| d.date.as_str() <= end.as_str());
                info!("Filtering to {}", end);
            }

            if days.is_empty() {
                anyhow::bail!("No cached data found for date range. Run 'precompute' first.");
            }

            info!("Loaded {} days of data", days.len());

            // Run the sweep
            let results = smart_lvn::run_multi_sweep(
                &days,
                &delta_values,
                &trailing_stop_values,
                &impulse_size_values,
                &time_window_values,
            );

            // Print results
            smart_lvn::print_multi_sweep_results(&results, top_n);
        }
        Commands::AnalyzeDelta { cache_dir, min_impulse_size } => {
            info!("=== DELTA DISTRIBUTION ANALYSIS ===");
            info!("Finding optimal delta threshold for impulse_size={}", min_impulse_size);

            let days = precompute::load_all_cached(&cache_dir, None)?;

            if days.is_empty() {
                anyhow::bail!("No cached data found. Run 'precompute' first.");
            }

            info!("Loaded {} days of data", days.len());

            smart_lvn::analyze_delta_distribution(&days, min_impulse_size);
        }
        Commands::FetchDate { date, cache_dir, symbol } => {
            info!("=== FETCH DATE FROM DATABENTO ===");
            info!("Date: {}, Symbol: {}", date, symbol);

            // Parse date
            let year: i32 = date[0..4].parse()?;
            let month: u32 = date[4..6].parse()?;
            let day: u32 = date[6..8].parse()?;

            let naive_date = chrono::NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| anyhow::anyhow!("Invalid date: {}", date))?;

            info!("Fetching trades for {}", naive_date);

            // Fetch from Databento (already in async context)
            let day_data = fetch_date::fetch_and_precompute(
                &std::env::var("DATABENTO_API_KEY")?,
                &symbol,
                naive_date,
            ).await?;

            info!("Fetched {} bars, {} LVNs", day_data.bars_1s.len(), day_data.lvn_levels.len());

            // Save to cache
            std::fs::create_dir_all(&cache_dir)?;
            let cache_path = cache_dir.join(format!("{}.json.zst", date));

            let file = std::fs::File::create(&cache_path)?;
            let encoder = zstd::stream::Encoder::new(file, 3)?;
            let writer = std::io::BufWriter::new(encoder.auto_finish());
            serde_json::to_writer(writer, &day_data)?;

            info!("Saved to {}", cache_path.display());
        }
        Commands::BatchFetch { start, end, cache_dir, symbol } => {
            info!("=== BATCH FETCH FROM DATABENTO ===");
            info!("Base symbol: {}, Date range: {} to {}", symbol, start, end);

            // Parse dates
            let start_date = chrono::NaiveDate::parse_from_str(&start, "%Y%m%d")
                .context("Invalid start date")?;
            let end_date = chrono::NaiveDate::parse_from_str(&end, "%Y%m%d")
                .context("Invalid end date")?;

            std::fs::create_dir_all(&cache_dir)?;

            let api_key = std::env::var("DATABENTO_API_KEY")?;
            let mut current = start_date;
            let mut fetched = 0;
            let mut skipped = 0;

            while current <= end_date {
                // Skip weekends
                if current.weekday() == chrono::Weekday::Sat || current.weekday() == chrono::Weekday::Sun {
                    current = current + chrono::Duration::days(1);
                    continue;
                }

                let date_str = current.format("%Y%m%d").to_string();
                let cache_path = cache_dir.join(format!("{}.json.zst", date_str));

                // Skip if already cached
                if cache_path.exists() {
                    info!("Skipping {} (already cached)", date_str);
                    skipped += 1;
                    current = current + chrono::Duration::days(1);
                    continue;
                }

                // Get the correct contract symbol for this date
                let contract_symbol = get_front_month_contract(&symbol, current);
                info!("Fetching {} ({})...", date_str, contract_symbol);

                match fetch_date::fetch_and_precompute(&api_key, &contract_symbol, current).await {
                    Ok(day_data) => {
                        if day_data.bars_1s.is_empty() {
                            warn!("  → No data for {} (holiday?)", date_str);
                        } else {
                            let file = std::fs::File::create(&cache_path)?;
                            let encoder = zstd::stream::Encoder::new(file, 3)?;
                            let writer = std::io::BufWriter::new(encoder.auto_finish());
                            serde_json::to_writer(writer, &day_data)?;
                            info!("  → {} bars, {} LVNs", day_data.bars_1s.len(), day_data.lvn_levels.len());
                            fetched += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch {}: {}", date_str, e);
                    }
                }

                current = current + chrono::Duration::days(1);

                // Rate limit: don't hammer Databento
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }

            info!("=== BATCH COMPLETE ===");
            info!("Fetched: {}, Skipped: {}", fetched, skipped);
        }
        Commands::IbTest => {
            ib_execution::run_ib_demo()?;
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

            let config = trader::LiveConfig {
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
                slippage: 0.0, // Live trading - actual slippage from fills
                commission: 0.0, // Live trading - actual commission from broker
                max_win_cap: 0.0, // Disabled for live
                volatility_slippage_factor: 0.0, // Disabled for live
                outlier_threshold: 0.0, // Disabled for live
            };

            let ib_config = ib_execution::IbConfig {
                host,
                port,
                client_id,
            };

            ib_execution::run_ib_live(config, ib_config, paper_mode)?;
        }
        Commands::IbDataTest { symbol, duration } => {
            ib_execution::run_ib_data_test(&symbol, duration)?;
        }
        Commands::IbFuturesTest { duration } => {
            ib_execution::run_ib_futures_test(duration)?;
        }
        Commands::DatabentoIbLive {
            mode, contract_symbol, trade_log, client_id, contracts, cache_dir, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, max_lvn_ratio, level_tolerance,
            starting_balance, max_daily_losses, daily_loss_limit,
            breakout_threshold, min_impulse_size, min_impulse_score,
            max_impulse_bars, max_hunting_bars, max_retrace_ratio,
        } => {
            let mode_lower = mode.to_lowercase();
            let observe_mode = mode_lower == "observe";
            let paper_mode = mode_lower != "live";

            if !paper_mode && !observe_mode {
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

            let config = trader::LiveConfig {
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
                // Live trading: no artificial caps/adjustments
                max_win_cap: 0.0,
                volatility_slippage_factor: 0.0,
                outlier_threshold: 0.0,
            };

            let sm_config = state_machine::StateMachineConfig {
                breakout_threshold,
                min_impulse_size,
                min_impulse_score,
                max_impulse_bars,
                max_hunting_bars,
                max_retrace_ratio,
                min_bars_before_switch: 60, // 1 minute before switching to new breakout
            };

            if observe_mode {
                // Observe mode: no IB connection, just track trades internally
                databento_ib_live::run_observe_mode(
                    api_key,
                    contract_symbol,
                    config,
                    sm_config,
                    trade_log,
                ).await?;
            } else {
                // Paper or live mode: connect to IB for execution
                let ib_config = ib_execution::IbConfig {
                    client_id,
                    ..ib_execution::IbConfig::default()
                };
                databento_ib_live::run_databento_ib_live(
                    api_key,
                    contract_symbol,
                    config,
                    sm_config,
                    ib_config,
                    paper_mode
                ).await?;
            }
        }
        Commands::TopstepLive {
            contract_symbol, trade_log, contracts, cache_dir, take_profit, trailing_stop, stop_buffer,
            start_hour, start_minute, end_hour, end_minute,
            min_delta, starting_balance, max_daily_losses, daily_loss_limit,
            breakout_threshold, min_impulse_size, min_impulse_score,
            max_impulse_bars, max_hunting_bars, max_retrace_ratio,
        } => {
            use hitthebid::topstepx::{TopstepClient, TopstepExecutor};

            println!("═══════════════════════════════════════════════════════════");
            println!("           TOPSTEP LIVE TRADING                            ");
            println!("═══════════════════════════════════════════════════════════");
            println!();
            println!("Symbol: {}", contract_symbol);
            println!("Contracts: {}", contracts);
            println!("Trading Hours: {:02}:{:02} - {:02}:{:02} ET", start_hour, start_minute, end_hour, end_minute);
            println!();

            let databento_key = std::env::var("DATABENTO_API_KEY")
                .context("DATABENTO_API_KEY not set")?;

            // Create TopstepX client and executor
            info!("Connecting to TopstepX API...");
            let client = TopstepClient::from_env()
                .context("Failed to create TopstepX client. Check TOPSTEP_USERNAME and TOPSTEP_API_KEY")?;

            // Extract base symbol (e.g., NQ from NQH6)
            let base_symbol = &contract_symbol[..2];
            let mut executor = TopstepExecutor::new(client, base_symbol).await
                .context("Failed to initialize TopstepX executor")?;

            info!("TopstepX executor ready");

            let config = trader::LiveConfig {
                symbol: base_symbol.to_string(),
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
                max_lvn_ratio: 0.4,
                level_tolerance: 3.0,
                starting_balance,
                max_daily_losses,
                daily_loss_limit,
                point_value: 20.0,
                slippage: 0.0,
                commission: 0.0,
                max_win_cap: 0.0,
                volatility_slippage_factor: 0.0,
                outlier_threshold: 0.0,
            };

            let sm_config = state_machine::StateMachineConfig {
                breakout_threshold,
                min_impulse_size,
                min_impulse_score,
                max_impulse_bars,
                max_hunting_bars,
                max_retrace_ratio,
                min_bars_before_switch: 60,
            };

            // Run in observe mode with TopstepX execution
            databento_ib_live::run_topstep_mode(
                databento_key,
                contract_symbol,
                config,
                sm_config,
                executor,
                trade_log,
            ).await?;
        }
        Commands::DatabentoTest { symbol, duration } => {
            use databento::{
                dbn::{Schema, SType, TradeMsg},
                live::Subscription,
                LiveClient,
            };
            use chrono::DateTime;

            let api_key = std::env::var("DATABENTO_API_KEY")
                .context("DATABENTO_API_KEY not set")?;

            println!("═══════════════════════════════════════════════════════════");
            println!("           DATABENTO LIVE DATA TEST                        ");
            println!("═══════════════════════════════════════════════════════════");
            println!();
            println!("Symbol: {}", symbol);
            println!("Duration: {} seconds (0 = indefinite)", duration);
            println!();

            println!("Connecting to Databento...");
            let mut client = LiveClient::builder()
                .key(api_key)?
                .dataset("GLBX.MDP3")
                .build()
                .await
                .context("Failed to connect to Databento")?;

            println!("Connected! Subscribing to {}...", symbol);

            let subscription = Subscription::builder()
                .symbols(vec![symbol.clone()])
                .schema(Schema::Trades)
                .stype_in(SType::RawSymbol)
                .build();

            client.subscribe(subscription).await
                .context("Failed to subscribe")?;

            client.start().await.context("Failed to start stream")?;

            println!("Subscribed! Waiting for trades...\n");

            let start = std::time::Instant::now();
            let mut trade_count = 0u64;
            let mut total_volume = 0u64;

            while let Some(record) = client.next_record().await? {
                if let Some(trade) = record.get::<TradeMsg>() {
                    trade_count += 1;
                    let price = trade.price as f64 / 1_000_000_000.0;
                    let size = trade.size as u64;
                    total_volume += size;

                    let side = match trade.side as u8 {
                        b'A' | b'a' => "BUY ",
                        b'B' | b'b' => "SELL",
                        _ => "??? ",
                    };

                    let ts = DateTime::from_timestamp_nanos(trade.hd.ts_event as i64);

                    println!(
                        "[{}] {} {:>2} @ {:.2}  (total: {} trades, {} contracts)",
                        ts.format("%H:%M:%S%.3f"),
                        side,
                        size,
                        price,
                        trade_count,
                        total_volume
                    );
                }

                // Check duration limit
                if duration > 0 && start.elapsed().as_secs() >= duration {
                    println!("\nDuration limit reached.");
                    break;
                }
            }

            println!("\n═══════════════════════════════════════════════════════════");
            println!("Total trades: {}", trade_count);
            println!("Total volume: {} contracts", total_volume);
            println!("═══════════════════════════════════════════════════════════");
        }
        Commands::Sweep {
            cache_dir,
            output,
            start_hour,
            start_minute,
            end_hour,
            end_minute,
            min_delta,
            trailing_stop,
            take_profit,
            stop_buffer,
            max_hunting_bars,
            min_impulse_score,
        } => {
            // Parse comma-separated values
            let min_delta_values: Vec<i64> = min_delta
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let trailing_stop_values: Vec<f64> = trailing_stop
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let take_profit_values: Vec<f64> = take_profit
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let stop_buffer_values: Vec<f64> = stop_buffer
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let max_hunting_bars_values: Vec<usize> = max_hunting_bars
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let min_impulse_score_values: Vec<u8> = min_impulse_score
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            // Fixed parameters
            let max_lvn_ratio_values = vec![0.25];
            let min_impulse_size_values = vec![20.0];
            let breakout_threshold_values = vec![2.0];

            let combinations = sweep::generate_combinations(
                &min_delta_values,
                &max_lvn_ratio_values,
                &min_impulse_size_values,
                &min_impulse_score_values,
                &take_profit_values,
                &trailing_stop_values,
                &stop_buffer_values,
                &breakout_threshold_values,
                &max_hunting_bars_values,
            );

            println!("═══════════════════════════════════════════════════════════");
            println!("              PARAMETER SWEEP                              ");
            println!("═══════════════════════════════════════════════════════════");
            println!();
            println!("Parameters:");
            println!("  min_delta: {:?}", min_delta_values);
            println!("  trailing_stop: {:?}", trailing_stop_values);
            println!("  take_profit: {:?}", take_profit_values);
            println!("  stop_buffer: {:?}", stop_buffer_values);
            println!("  max_hunting_bars: {:?}", max_hunting_bars_values);
            println!("  min_impulse_score: {:?}", min_impulse_score_values);
            println!();
            println!("Total combinations: {}", combinations.len());
            println!("Trading hours: {:02}:{:02} - {:02}:{:02} ET", start_hour, start_minute, end_hour, end_minute);
            println!();

            sweep::run_sweep(
                cache_dir,
                output,
                combinations,
                start_hour,
                start_minute,
                end_hour,
                end_minute,
            )?;
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
