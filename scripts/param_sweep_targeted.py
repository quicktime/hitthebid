#!/usr/bin/env python3
"""
Targeted Parameter Sweep - Based on quick sweep findings

Key findings:
- min_impulse_score=4 is essential (not negotiable)
- delta=25 performed best, test around it
- trailing_stop=6 performed best
- lvn_ratio has minimal impact (fix at 0.25)
- take_profit has minimal impact

This sweep tests ~200 combinations in ~30 minutes.
"""

import subprocess
import csv
import re
import itertools
from datetime import datetime

# Targeted parameter ranges
PARAM_RANGES = {
    # Focus on delta values around the sweet spot
    'min_delta': [15, 20, 25, 30, 35, 40],

    # LVN ratio - fixed based on findings (minimal impact)
    'max_lvn_ratio': [0.25],

    # Impulse - score=4 is essential, size=20 works well
    'min_impulse_size': [20.0],
    'min_impulse_score': [4],

    # Trade management
    'take_profit': [25, 30, 35],
    'trailing_stop': [5, 6, 7, 8],
    'stop_buffer': [2, 3],

    # State machine
    'breakout_threshold': [2.0],
    'max_hunting_bars': [600, 900],
}

# Fixed parameters
FIXED_PARAMS = {
    'cache_dir': 'cache_2025',
    'contracts': 1,
    'start_hour': 9,
    'start_minute': 30,
    'end_hour': 16,
    'end_minute': 0,
    'level_tolerance': 2.0,
    'starting_balance': 50000,
    'max_impulse_bars': 300,
    'max_retrace_ratio': 0.7,
    'max_win_cap': 0,
    'outlier_threshold': 0,
}


def run_backtest(params: dict) -> dict:
    """Run a single backtest with given parameters and return results."""

    cmd = [
        './target/release/pipeline', 'replay-realtime',
        '--cache-dir', str(params['cache_dir']),
        '--contracts', str(params['contracts']),
        '--take-profit', str(params['take_profit']),
        '--trailing-stop', str(params['trailing_stop']),
        '--stop-buffer', str(params['stop_buffer']),
        '--start-hour', str(params['start_hour']),
        '--start-minute', str(params['start_minute']),
        '--end-hour', str(params['end_hour']),
        '--end-minute', str(params['end_minute']),
        '--min-delta', str(params['min_delta']),
        '--max-lvn-ratio', str(params['max_lvn_ratio']),
        '--level-tolerance', str(params['level_tolerance']),
        '--starting-balance', str(params['starting_balance']),
        '--breakout-threshold', str(params['breakout_threshold']),
        '--min-impulse-size', str(params['min_impulse_size']),
        '--max-impulse-bars', str(params['max_impulse_bars']),
        '--max-hunting-bars', str(params['max_hunting_bars']),
        '--min-impulse-score', str(params['min_impulse_score']),
        '--max-retrace-ratio', str(params['max_retrace_ratio']),
        '--max-win-cap', str(params['max_win_cap']),
        '--outlier-threshold', str(params['outlier_threshold']),
    ]

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        output = result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return {'error': 'timeout'}
    except Exception as e:
        return {'error': str(e)}

    return parse_output(output)


def parse_output(output: str) -> dict:
    """Parse backtest output and extract metrics."""

    results = {
        'total_trades': 0,
        'wins': 0,
        'losses': 0,
        'breakevens': 0,
        'win_rate': 0.0,
        'profit_factor': 0.0,
        'sharpe_ratio': 0.0,
        'avg_win': 0.0,
        'avg_loss': 0.0,
        'total_pnl': 0.0,
        'max_drawdown': 0.0,
    }

    patterns = {
        'total_trades': r'Total Trades:\s+(\d+)',
        'wins': r'Wins:\s+(\d+)',
        'losses': r'Losses:\s+(\d+)',
        'breakevens': r'Breakevens:\s+(\d+)',
        'profit_factor': r'Profit Factor:\s+([\d.]+)',
        'sharpe_ratio': r'Sharpe Ratio:\s+([-\d.]+)',
        'avg_win': r'Avg Win:\s+([\d.]+)',
        'avg_loss': r'Avg Loss:\s+([-\d.]+)',
        'total_pnl': r'Total P&L:\s+([+-]?[\d.]+)',
        'max_drawdown': r'Max Drawdown:\s+\$([\d,.]+)',
    }

    for key, pattern in patterns.items():
        match = re.search(pattern, output)
        if match:
            value = match.group(1).replace(',', '')
            if key in ['total_trades', 'wins', 'losses', 'breakevens']:
                results[key] = int(value)
            else:
                results[key] = float(value)

    if results['total_trades'] > 0:
        results['win_rate'] = results['wins'] / results['total_trades'] * 100

    return results


def generate_combinations(param_ranges: dict) -> list:
    """Generate all parameter combinations."""

    keys = list(param_ranges.keys())
    values = [param_ranges[k] for k in keys]

    combinations = []
    for combo in itertools.product(*values):
        params = dict(zip(keys, combo))
        params.update(FIXED_PARAMS)
        combinations.append(params)

    return combinations


def main():
    output_file = 'sweep_targeted.csv'
    combinations = generate_combinations(PARAM_RANGES)

    print(f"Targeted Parameter Sweep")
    print(f"  Total combinations: {len(combinations)}")
    print(f"  Output file: {output_file}")
    print(f"  Estimated time: {len(combinations) * 8 / 60:.1f} minutes")
    print()

    fieldnames = [
        'min_delta', 'max_lvn_ratio', 'min_impulse_size', 'min_impulse_score',
        'take_profit', 'trailing_stop', 'stop_buffer', 'breakout_threshold', 'max_hunting_bars',
        'total_trades', 'wins', 'losses', 'breakevens', 'win_rate',
        'profit_factor', 'sharpe_ratio', 'avg_win', 'avg_loss', 'total_pnl', 'max_drawdown',
        'rr_ratio', 'expectancy',
    ]

    with open(output_file, 'w', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()

        start_time = datetime.now()

        for i, params in enumerate(combinations):
            elapsed = (datetime.now() - start_time).total_seconds()
            if i > 0:
                eta = elapsed / i * (len(combinations) - i)
                eta_str = f"{eta/60:.1f}min"
            else:
                eta_str = "..."

            print(f"\r[{i+1}/{len(combinations)}] d={params['min_delta']} sz={params['min_impulse_size']} "
                  f"tp={params['take_profit']} tr={params['trailing_stop']} sb={params['stop_buffer']} "
                  f"hunt={params['max_hunting_bars']} ETA: {eta_str}      ", end='', flush=True)

            results = run_backtest(params)

            if 'error' in results:
                print(f"\n  Error: {results['error']}")
                continue

            if results['avg_loss'] != 0:
                results['rr_ratio'] = abs(results['avg_win'] / results['avg_loss'])
            else:
                results['rr_ratio'] = 0

            if results['total_trades'] > 0:
                win_pct = results['wins'] / results['total_trades']
                loss_pct = results['losses'] / results['total_trades']
                results['expectancy'] = (win_pct * results['avg_win']) - (loss_pct * abs(results['avg_loss']))
            else:
                results['expectancy'] = 0

            row = {k: params.get(k, results.get(k)) for k in fieldnames}
            row.update(results)
            writer.writerow(row)
            f.flush()

    print(f"\n\nSweep complete! Results saved to {output_file}")
    print(f"Total time: {(datetime.now() - start_time).total_seconds() / 60:.1f} minutes")

    # Quick summary
    print("\n--- Quick Summary ---")
    import csv as csv_mod
    with open(output_file, 'r') as f:
        reader = csv_mod.DictReader(f)
        results = list(reader)

    profitable = [r for r in results if float(r['total_pnl']) > 0 and int(r['total_trades']) >= 10]
    profitable.sort(key=lambda x: float(x['sharpe_ratio']), reverse=True)

    print(f"Profitable configs (>=10 trades): {len(profitable)}")
    if profitable:
        print("\nTop 10 by Sharpe:")
        for i, r in enumerate(profitable[:10], 1):
            print(f"  {i}. Sharpe={float(r['sharpe_ratio']):.2f} PF={float(r['profit_factor']):.2f} "
                  f"P&L={float(r['total_pnl']):+.1f} Trades={r['total_trades']} WR={float(r['win_rate']):.1f}% "
                  f"| d={r['min_delta']} sz={r['min_impulse_size']} tr={r['trailing_stop']} sb={r['stop_buffer']}")


if __name__ == '__main__':
    main()
