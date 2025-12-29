use rand::Rng;
use rand_distr::{Distribution, Normal};

/// Configuration for a trading strategy
#[derive(Clone)]
pub struct StrategyConfig {
    pub name: String,
    pub win_rate: f64,
    pub avg_win: f64,      // dollars
    pub avg_loss: f64,     // dollars (positive)
    pub peak_ratio: f64,   // unrealized peak / realized (e.g., 1.25)
    pub win_std: f64,      // standard deviation of wins
}

/// Results from Monte Carlo simulation
#[derive(Default)]
pub struct SimulationResults {
    pub passed: usize,
    pub failed_dd: usize,
    pub failed_max_trades: usize,
    pub trades_to_pass: Vec<usize>,
    pub final_buffers: Vec<f64>,
    pub min_buffers: Vec<f64>,
}

/// Apex account parameters
pub struct ApexParams {
    pub starting_balance: f64,
    pub starting_dd_threshold: f64,
    pub profit_target: f64,
    pub dd_lock_threshold: f64,
    pub max_trades: usize,
}

impl Default for ApexParams {
    fn default() -> Self {
        // 50K account
        Self {
            starting_balance: 50000.0,
            starting_dd_threshold: 47500.0,  // $2,500 trailing DD
            profit_target: 53000.0,           // Need to reach this
            dd_lock_threshold: 53000.0,       // Rithmic: DD locks here
            max_trades: 100,
        }
    }
}

/// Elite Trader Funding Static account parameters
/// ALL Static accounts (10K, 25K, 50K) have SAME rules:
/// - $2,000 max drawdown (static, never trails)
/// - $4,000 profit target
/// - Max 4 NQ / 40 MNQ contracts
/// - No daily loss limit
pub struct ETFStaticParams {
    pub starting_balance: f64,
    pub min_balance: f64,       // Static DD threshold (never moves)
    pub profit_target: f64,
    pub max_trades: usize,
}

impl ETFStaticParams {
    /// ETF Static: All sizes have same DD ($2,000) and target ($4,000)
    pub fn new_etf_static() -> Self {
        Self {
            starting_balance: 50000.0,
            min_balance: 48000.0,       // $2,000 static DD
            profit_target: 54000.0,     // $4,000 profit target
            max_trades: 300,
        }
    }

    /// MFF 30K Static Pro: $2,500 DD, $4,200 target
    pub fn new_mff_static_pro() -> Self {
        Self {
            starting_balance: 30000.0,
            min_balance: 27500.0,       // $2,500 static DD
            profit_target: 34200.0,     // $4,200 profit target
            max_trades: 300,
        }
    }

    /// MFF 30K Static Standard: $1,500 DD, $2,500 target
    pub fn new_mff_static_standard() -> Self {
        Self {
            starting_balance: 30000.0,
            min_balance: 28500.0,       // $1,500 static DD
            profit_target: 32500.0,     // $2,500 profit target
            max_trades: 300,
        }
    }
}

/// Elite Trader Funding EOD account parameters
/// ALL EOD accounts (50K, 100K, 150K) have SAME rules:
/// - $2,000 max drawdown (trails at EOD based on realized profit)
/// - $3,000 profit target
/// - $1,100 daily loss limit
/// - Max 8 NQ / 80 MNQ contracts
pub struct ETFEodParams {
    pub starting_balance: f64,
    pub max_drawdown: f64,      // EOD trailing DD amount
    pub profit_target: f64,
    pub daily_loss_limit: f64,
    pub max_trades: usize,
}

impl ETFEodParams {
    pub fn new_eod() -> Self {
        Self {
            starting_balance: 50000.0,
            max_drawdown: 2000.0,       // $2,000 EOD trailing DD
            profit_target: 53000.0,     // $3,000 profit target
            daily_loss_limit: 1100.0,   // $1,100 daily loss limit
            max_trades: 300,
        }
    }
}

/// Run Monte Carlo simulation for ETF Static DD eval
pub fn simulate_etf_static(
    config: &StrategyConfig,
    params: &ETFStaticParams,
    num_simulations: usize,
) -> SimulationResults {
    let mut results = SimulationResults::default();
    let mut rng = rand::thread_rng();

    let win_dist = Normal::new(config.avg_win, config.win_std).unwrap();
    let loss_dist = Normal::new(config.avg_loss, config.avg_loss * 0.3).unwrap();

    for _ in 0..num_simulations {
        let mut balance = params.starting_balance;
        let mut trades = 0usize;
        let mut min_buffer = balance - params.min_balance;

        loop {
            trades += 1;

            if rng.gen::<f64>() < config.win_rate {
                // Winner - peak ratio doesn't matter with static DD!
                let win_amount = win_dist.sample(&mut rng).max(config.avg_win * 0.3);
                balance += win_amount;
            } else {
                // Loser
                let loss_amount = loss_dist.sample(&mut rng).max(config.avg_loss * 0.5);
                balance -= loss_amount;
            }

            let buffer = balance - params.min_balance;
            min_buffer = min_buffer.min(buffer);

            // Check if blown (static DD - never moves)
            if balance <= params.min_balance {
                results.failed_dd += 1;
                break;
            }

            // Check if passed
            if balance >= params.profit_target {
                results.passed += 1;
                results.trades_to_pass.push(trades);
                results.final_buffers.push(balance - params.min_balance);
                results.min_buffers.push(min_buffer);
                break;
            }

            // Max trades
            if trades >= params.max_trades {
                results.failed_max_trades += 1;
                break;
            }
        }
    }

    results
}

/// Run Monte Carlo simulation for Apex eval
pub fn simulate_eval(
    config: &StrategyConfig,
    apex: &ApexParams,
    num_simulations: usize,
) -> SimulationResults {
    let mut results = SimulationResults::default();
    let mut rng = rand::thread_rng();

    let win_dist = Normal::new(config.avg_win, config.win_std).unwrap();
    let loss_dist = Normal::new(config.avg_loss, config.avg_loss * 0.2).unwrap();

    for _ in 0..num_simulations {
        let mut balance = apex.starting_balance;
        let mut dd_threshold = apex.starting_dd_threshold;
        let mut dd_locked = false;
        let mut trades = 0usize;
        let mut min_buffer = balance - dd_threshold;

        loop {
            trades += 1;

            if rng.gen::<f64>() < config.win_rate {
                // Winner
                let win_amount = win_dist.sample(&mut rng).max(config.avg_win * 0.3);
                let peak_amount = win_amount * config.peak_ratio;

                balance += win_amount;

                if !dd_locked {
                    dd_threshold += peak_amount;
                    if dd_threshold >= apex.dd_lock_threshold {
                        dd_threshold = apex.dd_lock_threshold;
                        dd_locked = true;
                    }
                }
            } else {
                // Loser
                let loss_amount = loss_dist.sample(&mut rng).max(config.avg_loss * 0.5);
                balance -= loss_amount;
            }

            let buffer = balance - dd_threshold;
            min_buffer = min_buffer.min(buffer);

            // Check if blown
            if balance <= dd_threshold {
                results.failed_dd += 1;
                break;
            }

            // Check if passed
            if balance >= apex.profit_target {
                results.passed += 1;
                results.trades_to_pass.push(trades);
                results.final_buffers.push(balance - dd_threshold);
                results.min_buffers.push(min_buffer);
                break;
            }

            // Max trades
            if trades >= apex.max_trades {
                results.failed_max_trades += 1;
                break;
            }
        }
    }

    results
}

/// Calculate statistics from results
pub fn print_results(config: &StrategyConfig, results: &SimulationResults, num_sims: usize) {
    println!("\n{}", "=".repeat(60));
    println!("CONFIG: {}", config.name);
    println!("{}", "=".repeat(60));
    println!("Win Rate: {:.1}%", config.win_rate * 100.0);
    println!("Avg Win: ${:.0} | Avg Loss: ${:.0}", config.avg_win, config.avg_loss);
    println!("Peak Ratio: {:.2}x", config.peak_ratio);
    println!();
    println!("SIMULATION RESULTS ({} runs):", num_sims);
    println!("{}", "-".repeat(40));

    let pass_rate = results.passed as f64 / num_sims as f64 * 100.0;
    let fail_dd_rate = results.failed_dd as f64 / num_sims as f64 * 100.0;
    let fail_max_rate = results.failed_max_trades as f64 / num_sims as f64 * 100.0;

    println!("  PASSED:           {} ({:.2}%)", results.passed, pass_rate);
    println!("  Failed (DD):      {} ({:.2}%)", results.failed_dd, fail_dd_rate);
    println!("  Failed (timeout): {} ({:.2}%)", results.failed_max_trades, fail_max_rate);

    if !results.trades_to_pass.is_empty() {
        let avg_trades: f64 = results.trades_to_pass.iter().map(|&x| x as f64).sum::<f64>()
            / results.trades_to_pass.len() as f64;

        let mut sorted_trades = results.trades_to_pass.clone();
        sorted_trades.sort();
        let med_trades = sorted_trades[sorted_trades.len() / 2];

        let avg_buffer: f64 = results.final_buffers.iter().sum::<f64>()
            / results.final_buffers.len() as f64;
        let avg_min_buffer: f64 = results.min_buffers.iter().sum::<f64>()
            / results.min_buffers.len() as f64;

        let mut sorted_min = results.min_buffers.clone();
        sorted_min.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let worst_min = sorted_min[0];
        let p5 = sorted_min[(sorted_min.len() as f64 * 0.05) as usize];
        let p10 = sorted_min[(sorted_min.len() as f64 * 0.10) as usize];

        println!();
        println!("  Avg trades to pass: {:.1}", avg_trades);
        println!("  Median trades:      {}", med_trades);
        println!("  Avg final buffer:   ${:.0}", avg_buffer);
        println!("  Avg min buffer:     ${:.0}", avg_min_buffer);
        println!("  Worst min buffer:   ${:.0}", worst_min);
        println!();
        println!("  5th percentile min buffer:  ${:.0}", p5);
        println!("  10th percentile min buffer: ${:.0}", p10);
    }
}

/// Run Monte Carlo simulation for ETF EOD (End of Day trailing) eval
pub fn simulate_etf_eod(
    config: &StrategyConfig,
    params: &ETFEodParams,
    num_simulations: usize,
) -> SimulationResults {
    let mut results = SimulationResults::default();
    let mut rng = rand::thread_rng();

    let win_dist = Normal::new(config.avg_win, config.win_std).unwrap();
    let loss_dist = Normal::new(config.avg_loss, config.avg_loss * 0.3).unwrap();

    for _ in 0..num_simulations {
        let mut balance = params.starting_balance;
        let mut dd_threshold = params.starting_balance - params.max_drawdown;
        let mut trades = 0usize;
        let mut min_buffer = params.max_drawdown;
        let mut daily_pnl = 0.0f64;

        loop {
            trades += 1;

            let trade_pnl = if rng.gen::<f64>() < config.win_rate {
                // Winner
                win_dist.sample(&mut rng).max(config.avg_win * 0.3)
            } else {
                // Loser
                -loss_dist.sample(&mut rng).max(config.avg_loss * 0.5)
            };

            daily_pnl += trade_pnl;
            balance += trade_pnl;

            // Check daily loss limit (calculated from prior day's close, so from start of day)
            if daily_pnl <= -params.daily_loss_limit {
                results.failed_dd += 1;
                break;
            }

            // At EOD, update trailing DD based on realized profit
            // Simulate ~1 trade per day on average, so update DD after each trade
            if balance > params.starting_balance {
                let profit = balance - params.starting_balance;
                dd_threshold = params.starting_balance + profit - params.max_drawdown;
            }
            daily_pnl = 0.0; // Reset for next "day"

            let buffer = balance - dd_threshold;
            min_buffer = min_buffer.min(buffer);

            // Check if blown (EOD trailing DD)
            if balance <= dd_threshold {
                results.failed_dd += 1;
                break;
            }

            // Check if passed
            if balance >= params.profit_target {
                results.passed += 1;
                results.trades_to_pass.push(trades);
                results.final_buffers.push(buffer);
                results.min_buffers.push(min_buffer);
                break;
            }

            // Max trades
            if trades >= params.max_trades {
                results.failed_max_trades += 1;
                break;
            }
        }
    }

    results
}

/// Run Monte Carlo for Elite Trader Funding - comparing Static vs EOD accounts
pub fn run_etf_monte_carlo() {
    const NUM_SIMULATIONS: usize = 100_000;

    println!("\n{}", "=".repeat(70));
    println!("ELITE TRADER FUNDING - STATIC vs EOD MONTE CARLO COMPARISON");
    println!("Simulations: {}", NUM_SIMULATIONS);
    println!("{}", "=".repeat(70));

    println!("\n{}", "-".repeat(70));
    println!("STRATEGY: LVN Retest (Full Year Backtest - 276 days)");
    println!("  55.4% WR, 52.77 pts avg win, 2.17 pts avg loss");
    println!("  ~0.84 trades/day (231 trades over 276 days)");
    println!("  PF: 45.28");
    println!("{}", "-".repeat(70));

    // Strategy configs for different contract sizes
    // Max 4 NQ for Static, Max 8 NQ for EOD
    let one_nq = StrategyConfig {
        name: "1 NQ ($20/pt)".to_string(),
        win_rate: 0.554,
        avg_win: 1055.40,   // 52.77 pts * $20
        avg_loss: 43.40,    // 2.17 pts * $20
        peak_ratio: 1.0,
        win_std: 400.0,
    };

    let two_nq = StrategyConfig {
        name: "2 NQ ($40/pt)".to_string(),
        win_rate: 0.554,
        avg_win: 2110.80,
        avg_loss: 86.80,
        peak_ratio: 1.0,
        win_std: 800.0,
    };

    let four_nq = StrategyConfig {
        name: "4 NQ ($80/pt) - MAX for Static".to_string(),
        win_rate: 0.554,
        avg_win: 4221.60,
        avg_loss: 173.60,
        peak_ratio: 1.0,
        win_std: 1600.0,
    };

    // ========== STATIC ACCOUNTS ==========
    println!("\n{}", "=".repeat(70));
    println!("STATIC ACCOUNTS (10K/25K/50K - all same rules)");
    println!("  $2,000 DD (never trails) | $4,000 profit target");
    println!("  No daily loss limit | Max 4 NQ / 40 MNQ");
    println!("  Cost: ~$46 (on sale)");
    println!("{}", "=".repeat(70));

    let static_params = ETFStaticParams::new_etf_static();

    let static_1nq = simulate_etf_static(&one_nq, &static_params, NUM_SIMULATIONS);
    print_etf_results(&one_nq, &static_1nq, NUM_SIMULATIONS);

    let static_2nq = simulate_etf_static(&two_nq, &static_params, NUM_SIMULATIONS);
    print_etf_results(&two_nq, &static_2nq, NUM_SIMULATIONS);

    let static_4nq = simulate_etf_static(&four_nq, &static_params, NUM_SIMULATIONS);
    print_etf_results(&four_nq, &static_4nq, NUM_SIMULATIONS);

    // ========== EOD ACCOUNTS ==========
    println!("\n{}", "=".repeat(70));
    println!("EOD ACCOUNTS (50K/100K/150K - all same rules)");
    println!("  $2,000 DD (trails at EOD) | $3,000 profit target");
    println!("  $1,100 daily loss limit | Max 8 NQ / 80 MNQ");
    println!("  Cost: ~$69 (on sale)");
    println!("{}", "=".repeat(70));

    let eod_params = ETFEodParams::new_eod();

    let eod_1nq = simulate_etf_eod(&one_nq, &eod_params, NUM_SIMULATIONS);
    print_etf_results(&one_nq, &eod_1nq, NUM_SIMULATIONS);

    let eod_2nq = simulate_etf_eod(&two_nq, &eod_params, NUM_SIMULATIONS);
    print_etf_results(&two_nq, &eod_2nq, NUM_SIMULATIONS);

    let eod_4nq = simulate_etf_eod(&four_nq, &eod_params, NUM_SIMULATIONS);
    print_etf_results(&four_nq, &eod_4nq, NUM_SIMULATIONS);

    // ========== MFF 30K STATIC ACCOUNTS ==========
    println!("\n{}", "=".repeat(70));
    println!("MYFUNDEDFUTURES 30K STATIC (Bot-Friendly!)");
    println!("  Bots explicitly allowed (July 2025 policy)");
    println!("  No activation fee | No consistency rule");
    println!("{}", "=".repeat(70));

    // MFF 30K Static Pro: $2,500 DD, $4,200 target, max 2 NQ
    println!("\n--- 30K Static Pro ($2,500 DD, $4,200 target) ---");
    let mff_pro = ETFStaticParams::new_mff_static_pro();

    // Max 2 NQ for MFF 30K
    let mff_1nq = simulate_etf_static(&one_nq, &mff_pro, NUM_SIMULATIONS);
    print_etf_results(&one_nq, &mff_1nq, NUM_SIMULATIONS);

    let mff_2nq = simulate_etf_static(&two_nq, &mff_pro, NUM_SIMULATIONS);
    print_etf_results(&two_nq, &mff_2nq, NUM_SIMULATIONS);

    // MFF 30K Static Standard: $1,500 DD, $2,500 target
    println!("\n--- 30K Static Standard ($1,500 DD, $2,500 target) ---");
    let mff_std = ETFStaticParams::new_mff_static_standard();

    let mff_std_1nq = simulate_etf_static(&one_nq, &mff_std, NUM_SIMULATIONS);
    print_etf_results(&one_nq, &mff_std_1nq, NUM_SIMULATIONS);

    let mff_std_2nq = simulate_etf_static(&two_nq, &mff_std, NUM_SIMULATIONS);
    print_etf_results(&two_nq, &mff_std_2nq, NUM_SIMULATIONS);

    // ========== COMPARISON SUMMARY ==========
    println!("\n{}", "=".repeat(70));
    println!("FULL COMPARISON: ETF vs MFF");
    println!("{}", "=".repeat(70));

    println!("\n  {:35} {:>12} {:>12} {:>10}", "Account / Contracts", "Pass Rate", "Avg Trades", "Est Days");
    println!("  {}", "-".repeat(73));

    let comparisons = [
        ("ETF Static / 1 NQ", &static_1nq),
        ("ETF Static / 2 NQ", &static_2nq),
        ("ETF Static / 4 NQ (max)", &static_4nq),
        ("ETF EOD / 1 NQ", &eod_1nq),
        ("ETF EOD / 2 NQ", &eod_2nq),
        ("MFF Static Pro / 1 NQ", &mff_1nq),
        ("MFF Static Pro / 2 NQ (max)", &mff_2nq),
        ("MFF Static Std / 1 NQ", &mff_std_1nq),
        ("MFF Static Std / 2 NQ (max)", &mff_std_2nq),
    ];

    for (name, results) in comparisons.iter() {
        let pass_rate = results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0;
        if !results.trades_to_pass.is_empty() {
            let avg_trades: f64 = results.trades_to_pass.iter().map(|&x| x as f64).sum::<f64>()
                / results.trades_to_pass.len() as f64;
            let est_days = (avg_trades / 0.84).ceil();
            println!("  {:35} {:>11.2}% {:>12.1} {:>10.0}", name, pass_rate, avg_trades, est_days);
        } else {
            println!("  {:35} {:>11.2}% {:>12} {:>10}", name, pass_rate, "N/A", "N/A");
        }
    }

    println!("\n{}", "-".repeat(70));
    println!("KEY DIFFERENCES:");
    println!("  ETF Static:     $2K DD, $4K target, needs bot approval");
    println!("  ETF EOD:        $2K DD, $3K target, $1.1K daily limit");
    println!("  MFF Static Pro: $2.5K DD, $4.2K target, BOTS ALLOWED");
    println!("  MFF Static Std: $1.5K DD, $2.5K target, BOTS ALLOWED");
    println!();
    println!("RECOMMENDATION for automated LVN Retest strategy:");
    println!("  MFF Static Pro + 2 NQ = BEST for bots");
    println!("    - Bots explicitly allowed (no approval needed)");
    println!("    - Higher DD ($2.5K vs $2K)");
    println!("    - No activation fee, no consistency rule");
    println!("{}", "-".repeat(70));
}

/// Print results for ETF static simulation
fn print_etf_results(config: &StrategyConfig, results: &SimulationResults, num_sims: usize) {
    println!("\n{}", "-".repeat(50));
    println!("CONFIG: {}", config.name);
    println!("{}", "-".repeat(50));

    let pass_rate = results.passed as f64 / num_sims as f64 * 100.0;
    let fail_dd_rate = results.failed_dd as f64 / num_sims as f64 * 100.0;
    let fail_max_rate = results.failed_max_trades as f64 / num_sims as f64 * 100.0;

    println!("  PASSED:           {} ({:.2}%)", results.passed, pass_rate);
    println!("  Failed (DD):      {} ({:.2}%)", results.failed_dd, fail_dd_rate);
    println!("  Failed (timeout): {} ({:.2}%)", results.failed_max_trades, fail_max_rate);

    if !results.trades_to_pass.is_empty() {
        let avg_trades: f64 = results.trades_to_pass.iter().map(|&x| x as f64).sum::<f64>()
            / results.trades_to_pass.len() as f64;

        let mut sorted_trades = results.trades_to_pass.clone();
        sorted_trades.sort();
        let med_trades = sorted_trades[sorted_trades.len() / 2];

        let avg_min_buffer: f64 = results.min_buffers.iter().sum::<f64>()
            / results.min_buffers.len() as f64;

        let mut sorted_min = results.min_buffers.clone();
        sorted_min.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p5 = sorted_min[(sorted_min.len() as f64 * 0.05) as usize];

        println!("  Avg trades to pass: {:.1} (~{:.0} days)", avg_trades, avg_trades / 0.84);
        println!("  Median trades:      {} (~{:.0} days)", med_trades, med_trades as f64 / 0.84);
        println!("  Avg min buffer:     ${:.0}", avg_min_buffer);
        println!("  5th %ile buffer:    ${:.0}", p5);
    }
}

/// Run Monte Carlo for both configs
pub fn run_monte_carlo() {
    const NUM_SIMULATIONS: usize = 100_000;

    println!("\n{}", "=".repeat(60));
    println!("APEX TRADER FUNDING - MONTE CARLO EVAL SIMULATION");
    println!("Platform: Rithmic (DD locks at $53K threshold)");
    println!("Account: 50K ($2,500 trailing DD, $3,000 profit target)");
    println!("Simulations: {}", NUM_SIMULATIONS);
    println!("{}", "=".repeat(60));

    let apex = ApexParams::default();

    // FULL YEAR BACKTEST: TP30 + Delta80 + Trail 1.5 (tight)
    // 238 trades, 34.5% WR, $65 avg win, $35 avg loss
    // Peak ratio: 2.00x (median), DD giveback: $283/win

    println!("\n{}", "-".repeat(60));
    println!("FULL YEAR BACKTEST (276 days, Trail 1.5):");
    println!("  34.5% WR, $65 avg win, $35 avg loss, 2.00x peak ratio");
    println!("  DD giveback per win: $283");
    println!("{}", "-".repeat(60));

    // 1 contract
    let one_ct = StrategyConfig {
        name: "1 CONTRACT (34.5% WR, $65 win, $35 loss, 2.0x peak)".to_string(),
        win_rate: 0.345,
        avg_win: 65.0,            // 3.23 pts * $20
        avg_loss: 35.0,           // 1.76 pts * $20
        peak_ratio: 2.0,          // Median from full year
        win_std: 25.0,
    };

    let one_ct_results = simulate_eval(&one_ct, &apex, NUM_SIMULATIONS);
    print_results(&one_ct, &one_ct_results, NUM_SIMULATIONS);

    // 2 contracts
    let two_ct = StrategyConfig {
        name: "2 CONTRACTS (34.5% WR, $130 win, $70 loss, 2.0x peak)".to_string(),
        win_rate: 0.345,
        avg_win: 130.0,
        avg_loss: 70.0,
        peak_ratio: 2.0,
        win_std: 50.0,
    };

    let two_ct_results = simulate_eval(&two_ct, &apex, NUM_SIMULATIONS);
    print_results(&two_ct, &two_ct_results, NUM_SIMULATIONS);

    // 3 contracts
    let three_ct = StrategyConfig {
        name: "3 CONTRACTS (34.5% WR, $195 win, $105 loss, 2.0x peak)".to_string(),
        win_rate: 0.345,
        avg_win: 195.0,
        avg_loss: 105.0,
        peak_ratio: 2.0,
        win_std: 75.0,
    };

    let three_ct_results = simulate_eval(&three_ct, &apex, NUM_SIMULATIONS);
    print_results(&three_ct, &three_ct_results, NUM_SIMULATIONS);

    // 5 contracts (50K allows 5 MNQ)
    let five_ct = StrategyConfig {
        name: "5 CONTRACTS (34.5% WR, $325 win, $175 loss, 2.0x peak)".to_string(),
        win_rate: 0.345,
        avg_win: 325.0,
        avg_loss: 175.0,
        peak_ratio: 2.0,
        win_std: 125.0,
    };

    let five_ct_results = simulate_eval(&five_ct, &apex, NUM_SIMULATIONS);
    print_results(&five_ct, &five_ct_results, NUM_SIMULATIONS);

    // Trail 1.0 for comparison (tightest, lowest peak ratio)
    println!("\n{}", "-".repeat(60));
    println!("TRAIL 1.0 (tightest - lowest peak ratio 1.67x):");
    println!("{}", "-".repeat(60));

    let trail1_ct = StrategyConfig {
        name: "TRAIL 1.0 - 5 CT (30% WR, $225 win, $175 loss, 1.67x peak)".to_string(),
        win_rate: 0.30,
        avg_win: 225.0,           // 2.23 pts * $20 * 5
        avg_loss: 175.0,          // 1.75 pts * $20 * 5
        peak_ratio: 1.67,         // Median from trail 1.0
        win_std: 80.0,
    };

    let trail1_results = simulate_eval(&trail1_ct, &apex, NUM_SIMULATIONS);
    print_results(&trail1_ct, &trail1_results, NUM_SIMULATIONS);

    // Summary
    println!("\n{}", "=".repeat(60));
    println!("SUMMARY - APEX EVAL PASS RATES (Full Year Data)");
    println!("{}", "=".repeat(60));

    println!("  Trail 1.5 (1 ct):  {:.2}%", one_ct_results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0);
    println!("  Trail 1.5 (2 ct):  {:.2}%", two_ct_results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0);
    println!("  Trail 1.5 (3 ct):  {:.2}%", three_ct_results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0);
    println!("  Trail 1.5 (5 ct):  {:.2}%", five_ct_results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0);
    println!("  Trail 1.0 (5 ct):  {:.2}%", trail1_results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0);

    if !five_ct_results.trades_to_pass.is_empty() {
        let avg_trades: f64 = five_ct_results.trades_to_pass.iter().map(|&x| x as f64).sum::<f64>()
            / five_ct_results.trades_to_pass.len() as f64;
        println!("\n  With 5 contracts (Trail 1.5):");
        println!("    Avg trades to pass: {:.0}", avg_trades);
        println!("    At ~0.86 trades/day = {:.0} trading days", avg_trades / 0.86);
    }

    println!("\n{}", "-".repeat(60));
    println!("FULL YEAR INSIGHTS:");
    println!("  - Peak ratio is HIGHER over full year (2.0x vs 1.0x in sample)");
    println!("  - Volatile market conditions cause large peak spikes");
    println!("  - Tight trail (1.0-1.5 pts) minimizes but doesn't eliminate issue");
    println!("  - Still viable but need realistic expectations");
    println!("{}", "-".repeat(60));
}
