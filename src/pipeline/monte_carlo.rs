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
pub struct ETFStaticParams {
    pub starting_balance: f64,
    pub min_balance: f64,       // Static DD threshold (never moves)
    pub profit_target: f64,
    pub max_trades: usize,
}

impl ETFStaticParams {
    pub fn new_100k() -> Self {
        Self {
            starting_balance: 100000.0,
            min_balance: 99375.0,    // $625 static DD
            profit_target: 102000.0,  // $2,000 profit target
            max_trades: 300,          // ~1 year of trading
        }
    }

    pub fn new_25k() -> Self {
        // 25K Static - proportional DD (~$156) and target (~$500)
        Self {
            starting_balance: 25000.0,
            min_balance: 24844.0,    // ~$156 static DD (proportional)
            profit_target: 25500.0,   // ~$500 profit target
            max_trades: 300,
        }
    }

    pub fn new_10k() -> Self {
        // 10K Static - proportional DD (~$62) and target (~$200)
        Self {
            starting_balance: 10000.0,
            min_balance: 9938.0,     // ~$62 static DD
            profit_target: 10200.0,   // ~$200 profit target
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

/// Run Monte Carlo for Elite Trader Funding Static accounts
pub fn run_etf_monte_carlo() {
    const NUM_SIMULATIONS: usize = 100_000;

    println!("\n{}", "=".repeat(70));
    println!("ELITE TRADER FUNDING - STATIC DD MONTE CARLO SIMULATION");
    println!("Simulations: {}", NUM_SIMULATIONS);
    println!("{}", "=".repeat(70));

    // BALANCED CONFIG from backtest_results.md (Full Year):
    // SL: 2.0 | TP: 30 | Trail: 6 | Delta: 60 | StopBuf: 1.5
    // 230 trades, 40.9% WR, 71.3 pts avg win, 2.25 pts avg loss
    // ~0.83 trades/day

    println!("\n{}", "-".repeat(70));
    println!("BALANCED CONFIG (Full Year Backtest - 276 days):");
    println!("  TP: 30 | Trail: 6 | Delta: 60 | StopBuf: 1.5 | LvlTol: 3");
    println!("  55.4% WR, 52.77 pts avg win, 2.17 pts avg loss");
    println!("  ~0.84 trades/day (231 trades over 276 days)");
    println!("  PF: 45.28, Total P&L: 7,483 pts ($150K/yr with 1 NQ)");
    println!("{}", "-".repeat(70));

    // ========== 100K STATIC ACCOUNT ==========
    println!("\n{}", "=".repeat(70));
    println!("100K STATIC ACCOUNT ($625 DD, $2,000 profit target)");
    println!("{}", "=".repeat(70));

    let etf_100k = ETFStaticParams::new_100k();

    // 1 NQ = $20/point
    // ACTUAL from backtest: 55.4% WR, 52.77 pts avg win, 2.17 pts avg loss
    // Avg Win: 52.77 pts * $20 = $1,055
    // Avg Loss: 2.17 pts * $20 = $43.40

    let one_nq = StrategyConfig {
        name: "1 NQ ($20/pt) - 55.4% WR, $1055 win, $43 loss".to_string(),
        win_rate: 0.554,
        avg_win: 1055.00,
        avg_loss: 43.40,
        peak_ratio: 1.0,
        win_std: 400.0,
    };

    let one_nq_results = simulate_etf_static(&one_nq, &etf_100k, NUM_SIMULATIONS);
    print_etf_results(&one_nq, &one_nq_results, NUM_SIMULATIONS);

    // 2 NQ
    let two_nq = StrategyConfig {
        name: "2 NQ ($40/pt) - 55.4% WR, $2110 win, $87 loss".to_string(),
        win_rate: 0.554,
        avg_win: 2110.00,
        avg_loss: 86.80,
        peak_ratio: 1.0,
        win_std: 800.0,
    };

    let two_nq_results = simulate_etf_static(&two_nq, &etf_100k, NUM_SIMULATIONS);
    print_etf_results(&two_nq, &two_nq_results, NUM_SIMULATIONS);

    // ========== 25K STATIC ACCOUNT ==========
    println!("\n{}", "=".repeat(70));
    println!("25K STATIC ACCOUNT (~$156 DD, ~$500 profit target)");
    println!("{}", "=".repeat(70));

    let etf_25k = ETFStaticParams::new_25k();

    // 5 MNQ = $10/point
    let five_mnq_25k = StrategyConfig {
        name: "5 MNQ ($10/pt) - 55.4% WR, $528 win, $22 loss".to_string(),
        win_rate: 0.554,
        avg_win: 527.70,
        avg_loss: 21.70,
        peak_ratio: 1.0,
        win_std: 200.0,
    };

    let five_mnq_25k_results = simulate_etf_static(&five_mnq_25k, &etf_25k, NUM_SIMULATIONS);
    print_etf_results(&five_mnq_25k, &five_mnq_25k_results, NUM_SIMULATIONS);

    // 3 MNQ - more conservative
    let three_mnq_25k = StrategyConfig {
        name: "3 MNQ ($6/pt) - 55.4% WR, $317 win, $13 loss".to_string(),
        win_rate: 0.554,
        avg_win: 316.62,
        avg_loss: 13.02,
        peak_ratio: 1.0,
        win_std: 120.0,
    };

    let three_mnq_25k_results = simulate_etf_static(&three_mnq_25k, &etf_25k, NUM_SIMULATIONS);
    print_etf_results(&three_mnq_25k, &three_mnq_25k_results, NUM_SIMULATIONS);

    // ========== SUMMARY ==========
    println!("\n{}", "=".repeat(70));
    println!("SUMMARY - ETF STATIC EVAL PASS RATES");
    println!("(Balanced Config: 55.4% WR, 52.77 pts win, 2.17 pts loss)");
    println!("{}", "=".repeat(70));

    println!("\n  {:25} {:>10} {:>12} {:>12}", "Account / Contracts", "Pass Rate", "Avg Trades", "Est. Days");
    println!("  {}", "-".repeat(62));

    let all_results = [
        ("100K / 1 NQ", &one_nq_results),
        ("100K / 2 NQ", &two_nq_results),
        ("25K / 5 MNQ", &five_mnq_25k_results),
        ("25K / 3 MNQ", &three_mnq_25k_results),
    ];

    for (name, results) in all_results.iter() {
        let pass_rate = results.passed as f64 / NUM_SIMULATIONS as f64 * 100.0;
        if !results.trades_to_pass.is_empty() {
            let avg_trades: f64 = results.trades_to_pass.iter().map(|&x| x as f64).sum::<f64>()
                / results.trades_to_pass.len() as f64;
            let est_days = avg_trades / 0.84;  // 231 trades / 276 days = 0.84 trades/day
            println!("  {:25} {:>9.2}% {:>12.1} {:>12.0}", name, pass_rate, avg_trades, est_days);
        } else {
            println!("  {:25} {:>9.2}% {:>12} {:>12}", name, pass_rate, "N/A", "N/A");
        }
    }

    println!("\n{}", "-".repeat(70));
    println!("RECOMMENDATION:");
    println!("  100K Static + 1 NQ = best balance of speed and safety");
    println!("  ~6 days to pass, 99.9%+ success rate");
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
