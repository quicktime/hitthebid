#!/usr/bin/env python3
"""
Analyze Parameter Sweep Results

Usage:
    python3 scripts/analyze_sweep.py results.csv
    python3 scripts/analyze_sweep.py results.csv --min-trades 20
    python3 scripts/analyze_sweep.py results.csv --sort-by sharpe_ratio
"""

import csv
import argparse
from collections import defaultdict
import statistics


def load_results(filepath: str) -> list:
    """Load results from CSV."""
    results = []
    with open(filepath, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            # Convert to appropriate types
            for key in row:
                try:
                    if '.' in row[key] or key in ['profit_factor', 'sharpe_ratio', 'avg_win', 'avg_loss', 'total_pnl', 'max_drawdown', 'rr_ratio', 'expectancy', 'win_rate']:
                        row[key] = float(row[key]) if row[key] else 0.0
                    else:
                        row[key] = int(row[key]) if row[key] else 0
                except (ValueError, TypeError):
                    pass
            results.append(row)
    return results


def filter_results(results: list, min_trades: int = 10, min_pf: float = 0.0) -> list:
    """Filter results by minimum criteria."""
    return [r for r in results if r['total_trades'] >= min_trades and r['profit_factor'] >= min_pf]


def sort_results(results: list, sort_by: str = 'total_pnl', reverse: bool = True) -> list:
    """Sort results by a metric."""
    return sorted(results, key=lambda x: x.get(sort_by, 0), reverse=reverse)


def print_top_results(results: list, n: int = 20):
    """Print top N results."""
    print(f"\n{'='*100}")
    print(f"TOP {n} CONFIGURATIONS")
    print(f"{'='*100}")

    header = f"{'#':>3} | {'Trades':>6} | {'Win%':>5} | {'PF':>5} | {'Sharpe':>6} | {'P&L':>8} | {'DD':>6} | {'R:R':>4} | Config"
    print(header)
    print("-" * 100)

    for i, r in enumerate(results[:n], 1):
        config = f"d={r['min_delta']} lvn={r['max_lvn_ratio']} sc={r['min_impulse_score']} tp={r['take_profit']} tr={r['trailing_stop']}"
        print(f"{i:>3} | {r['total_trades']:>6} | {r['win_rate']:>5.1f} | {r['profit_factor']:>5.2f} | {r['sharpe_ratio']:>6.2f} | {r['total_pnl']:>+8.1f} | {r['max_drawdown']:>6.0f} | {r['rr_ratio']:>4.1f} | {config}")


def analyze_parameter_impact(results: list, min_trades: int = 10):
    """Analyze impact of each parameter on performance."""

    print(f"\n{'='*100}")
    print("PARAMETER IMPACT ANALYSIS")
    print(f"{'='*100}")

    # Filter to only include results with enough trades
    valid = [r for r in results if r['total_trades'] >= min_trades]

    if not valid:
        print(f"No results with >= {min_trades} trades")
        return

    params = ['min_delta', 'max_lvn_ratio', 'min_impulse_score', 'take_profit', 'trailing_stop', 'stop_buffer', 'breakout_threshold', 'max_hunting_bars']

    for param in params:
        # Group by parameter value
        groups = defaultdict(list)
        for r in valid:
            groups[r[param]].append(r)

        print(f"\n{param}:")
        print(f"  {'Value':>10} | {'Count':>5} | {'Avg PF':>7} | {'Avg P&L':>9} | {'Avg Sharpe':>10} | {'Avg Trades':>10}")
        print(f"  {'-'*70}")

        for value in sorted(groups.keys()):
            g = groups[value]
            avg_pf = statistics.mean([r['profit_factor'] for r in g])
            avg_pnl = statistics.mean([r['total_pnl'] for r in g])
            avg_sharpe = statistics.mean([r['sharpe_ratio'] for r in g])
            avg_trades = statistics.mean([r['total_trades'] for r in g])
            print(f"  {value:>10} | {len(g):>5} | {avg_pf:>7.2f} | {avg_pnl:>+9.1f} | {avg_sharpe:>10.2f} | {avg_trades:>10.1f}")


def find_robust_configs(results: list, min_trades: int = 20):
    """Find configurations that are robust (profitable with good stats)."""

    print(f"\n{'='*100}")
    print("ROBUST CONFIGURATIONS (PF > 1.5, Sharpe > 1.0, Trades >= 20)")
    print(f"{'='*100}")

    robust = [r for r in results
              if r['total_trades'] >= min_trades
              and r['profit_factor'] > 1.5
              and r['sharpe_ratio'] > 1.0]

    if not robust:
        print("No robust configurations found with current criteria.")
        # Try looser criteria
        robust = [r for r in results
                  if r['total_trades'] >= 10
                  and r['profit_factor'] > 1.2
                  and r['total_pnl'] > 0]
        if robust:
            print(f"Showing {len(robust)} configs with looser criteria (PF > 1.2, P&L > 0, Trades >= 10):")

    for i, r in enumerate(sort_results(robust, 'sharpe_ratio')[:10], 1):
        print(f"\n  #{i}: Sharpe={r['sharpe_ratio']:.2f}, PF={r['profit_factor']:.2f}, P&L={r['total_pnl']:+.1f}, Trades={r['total_trades']}")
        print(f"      delta={r['min_delta']}, lvn={r['max_lvn_ratio']}, score={r['min_impulse_score']}")
        print(f"      tp={r['take_profit']}, trail={r['trailing_stop']}, stop_buf={r['stop_buffer']}")
        print(f"      hunting={r['max_hunting_bars']}, breakout={r['breakout_threshold']}")


def print_summary_stats(results: list):
    """Print overall summary statistics."""

    print(f"\n{'='*100}")
    print("SUMMARY STATISTICS")
    print(f"{'='*100}")

    print(f"\nTotal configurations tested: {len(results)}")

    profitable = [r for r in results if r['total_pnl'] > 0]
    print(f"Profitable configurations: {len(profitable)} ({len(profitable)/len(results)*100:.1f}%)")

    with_trades = [r for r in results if r['total_trades'] > 0]
    print(f"Configurations with trades: {len(with_trades)} ({len(with_trades)/len(results)*100:.1f}%)")

    if profitable:
        best = max(profitable, key=lambda x: x['total_pnl'])
        print(f"\nBest by P&L: {best['total_pnl']:+.1f} pts")
        print(f"  Config: delta={best['min_delta']}, lvn={best['max_lvn_ratio']}, score={best['min_impulse_score']}, tp={best['take_profit']}, trail={best['trailing_stop']}")

        best_pf = max([r for r in profitable if r['total_trades'] >= 10], key=lambda x: x['profit_factor'], default=None)
        if best_pf:
            print(f"\nBest by PF (min 10 trades): {best_pf['profit_factor']:.2f}")
            print(f"  Config: delta={best_pf['min_delta']}, lvn={best_pf['max_lvn_ratio']}, score={best_pf['min_impulse_score']}, tp={best_pf['take_profit']}, trail={best_pf['trailing_stop']}")

        best_sharpe = max([r for r in profitable if r['total_trades'] >= 10], key=lambda x: x['sharpe_ratio'], default=None)
        if best_sharpe:
            print(f"\nBest by Sharpe (min 10 trades): {best_sharpe['sharpe_ratio']:.2f}")
            print(f"  Config: delta={best_sharpe['min_delta']}, lvn={best_sharpe['max_lvn_ratio']}, score={best_sharpe['min_impulse_score']}, tp={best_sharpe['take_profit']}, trail={best_sharpe['trailing_stop']}")


def main():
    parser = argparse.ArgumentParser(description='Analyze parameter sweep results')
    parser.add_argument('input', help='Input CSV file from param_sweep.py')
    parser.add_argument('--min-trades', type=int, default=10, help='Minimum trades to consider')
    parser.add_argument('--sort-by', default='total_pnl', help='Sort metric (total_pnl, profit_factor, sharpe_ratio, etc.)')
    parser.add_argument('--top', type=int, default=20, help='Show top N results')
    args = parser.parse_args()

    print(f"Loading results from {args.input}...")
    results = load_results(args.input)
    print(f"Loaded {len(results)} configurations")

    # Summary
    print_summary_stats(results)

    # Filter and sort
    filtered = filter_results(results, min_trades=args.min_trades)
    sorted_results = sort_results(filtered, sort_by=args.sort_by)

    # Top results
    print_top_results(sorted_results, n=args.top)

    # Parameter impact
    analyze_parameter_impact(results, min_trades=args.min_trades)

    # Robust configs
    find_robust_configs(results, min_trades=args.min_trades)


if __name__ == '__main__':
    main()
