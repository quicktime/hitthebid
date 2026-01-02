#!/usr/bin/env python3
"""
Parameter Sweep Script for LVN Retest Strategy Optimization

Usage:
    python3 scripts/param_sweep.py --output results.csv
    python3 scripts/param_sweep.py --output results.csv --quick  # Quick test with fewer combos
"""

import subprocess
import csv
import re
import itertools
import argparse
from datetime import datetime
from pathlib import Path
import sys

# Parameter ranges to test
PARAM_RANGES = {
    # Signal parameters
    'min_delta': [10, 15, 20, 25, 30, 40, 50],
    'max_lvn_ratio': [0.15, 0.20, 0.25, 0.30],

    # Impulse parameters
    'min_impulse_size': [15.0, 20.0, 25.0, 30.0],
    'min_impulse_score': [3, 4],

    # Trade management
    'take_profit': [20, 25, 30, 35],
    'trailing_stop': [6, 8, 10],
    'stop_buffer': [2, 3],

    # State machine
    'breakout_threshold': [2.0, 3.0],
    'max_hunting_bars': [600, 900, 1200],
}

# Quick test with fewer combinations
QUICK_RANGES = {
    'min_delta': [15, 25, 40],
    'max_lvn_ratio': [0.20, 0.30],
    'min_impulse_size': [20.0],
    'min_impulse_score': [3, 4],
    'take_profit': [25, 30],
    'trailing_stop': [6, 8],
    'stop_buffer': [2],
    'breakout_threshold': [2.0],
    'max_hunting_bars': [600],
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

    # Calculate win rate
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
    parser = argparse.ArgumentParser(description='Parameter sweep for LVN strategy')
    parser.add_argument('--output', '-o', default='sweep_results.csv', help='Output CSV file')
    parser.add_argument('--quick', action='store_true', help='Quick test with fewer combinations')
    parser.add_argument('--resume', type=int, default=0, help='Resume from combination N')
    args = parser.parse_args()

    ranges = QUICK_RANGES if args.quick else PARAM_RANGES
    combinations = generate_combinations(ranges)

    print(f"Parameter Sweep Configuration:")
    print(f"  Total combinations: {len(combinations)}")
    print(f"  Output file: {args.output}")
    print(f"  Mode: {'Quick' if args.quick else 'Full'}")
    print()

    # CSV header
    fieldnames = [
        # Parameters
        'min_delta', 'max_lvn_ratio', 'min_impulse_size', 'min_impulse_score',
        'take_profit', 'trailing_stop', 'stop_buffer', 'breakout_threshold', 'max_hunting_bars',
        # Results
        'total_trades', 'wins', 'losses', 'breakevens', 'win_rate',
        'profit_factor', 'sharpe_ratio', 'avg_win', 'avg_loss', 'total_pnl', 'max_drawdown',
        # Computed
        'rr_ratio', 'expectancy',
    ]

    # Open CSV in append mode if resuming
    mode = 'a' if args.resume > 0 else 'w'
    with open(args.output, mode, newline='') as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        if args.resume == 0:
            writer.writeheader()

        start_time = datetime.now()

        for i, params in enumerate(combinations):
            if i < args.resume:
                continue

            # Progress
            elapsed = (datetime.now() - start_time).total_seconds()
            if i > args.resume:
                eta = elapsed / (i - args.resume) * (len(combinations) - i)
                eta_str = f"{eta/60:.1f}min"
            else:
                eta_str = "calculating..."

            print(f"\r[{i+1}/{len(combinations)}] delta={params['min_delta']} lvn={params['max_lvn_ratio']} "
                  f"score={params['min_impulse_score']} tp={params['take_profit']} trail={params['trailing_stop']} "
                  f"ETA: {eta_str}    ", end='', flush=True)

            # Run backtest
            results = run_backtest(params)

            if 'error' in results:
                print(f"\n  Error: {results['error']}")
                continue

            # Compute additional metrics
            if results['avg_loss'] != 0:
                results['rr_ratio'] = abs(results['avg_win'] / results['avg_loss'])
            else:
                results['rr_ratio'] = 0

            # Expectancy = (Win% * AvgWin) - (Loss% * AvgLoss)
            if results['total_trades'] > 0:
                win_pct = results['wins'] / results['total_trades']
                loss_pct = results['losses'] / results['total_trades']
                results['expectancy'] = (win_pct * results['avg_win']) - (loss_pct * abs(results['avg_loss']))
            else:
                results['expectancy'] = 0

            # Write row
            row = {k: params.get(k, results.get(k)) for k in fieldnames}
            row.update(results)
            writer.writerow(row)
            f.flush()

    print(f"\n\nSweep complete! Results saved to {args.output}")
    print(f"Total time: {(datetime.now() - start_time).total_seconds() / 60:.1f} minutes")


if __name__ == '__main__':
    main()
