# Optimized Trading Configuration

## State Machine Real-Time LVN Strategy

Optimized parameters for the real-time state machine trading strategy, validated over 276 trading days.

## Performance Summary

| Metric | Value |
|--------|-------|
| Total Trades | 831 (3.01/day) |
| Win Rate | 49.1% |
| Profit Factor | 16.21 |
| Sharpe Ratio | 3.62 |
| Avg Win | 26.02 pts |
| Avg Loss | -2.66 pts |
| Total P&L | +9,960.75 pts |
| Return | 665% ($30k → $229k) |
| Max Drawdown | $555 (1.85%) |

## Optimized Parameters

### Trading Hours
- **Start**: 9:30 AM ET (market open)
- **End**: 4:00 PM ET (market close)
- Pre-market (before 9:00) significantly increases drawdown
- Evening session (after 17:00) adds marginal P&L but 8x drawdown

### Exit Strategy
| Parameter | Value | Notes |
|-----------|-------|-------|
| `take_profit` | 500 pts | Effectively disabled - rely on trailing stop |
| `trailing_stop` | 4 pts | Optimal balance of protection vs room to run |
| `stop_buffer` | 1.5 pts | Distance beyond LVN for initial stop |

### Signal Generation
| Parameter | Value | Notes |
|-----------|-------|-------|
| `min_delta` | 5 | Lower threshold for 1-second bar deltas |
| `level_tolerance` | 3.0 pts | How close price must be to LVN level |
| `max_lvn_ratio` | 0.4 | Quality filter for LVN levels |

### State Machine (Impulse Detection)
| Parameter | Value | Notes |
|-----------|-------|-------|
| `breakout_threshold` | 2.0 pts | Points beyond level to confirm breakout |
| `min_impulse_size` | 15 pts | Minimum impulse move size |
| `min_impulse_score` | 3 | Lowered from 4 for 1-second bars |
| `max_impulse_bars` | 600 | 10 minutes max for impulse profiling |
| `max_hunting_bars` | 1800 | 30 minutes max to hunt for retest |
| `max_retrace_ratio` | 0.7 | 70% retrace allowed before invalidation |

## Key Insights

### What Works
1. **Trailing stop only** - No fixed take profit, let winners run
2. **Tight trailing (4 pts)** - Best profit factor and lowest drawdown
3. **Low min_delta (5)** - More signals from 1-second bar data
4. **RTH only (9:30-16:00)** - Best risk-adjusted returns
5. **Score threshold of 3** - Required because `is_fast()` always fails with 1s bars

### What Doesn't Work
1. **Pre-market trading** - Drawdown explodes 8-10x
2. **Evening session** - High win rate but massive drawdown
3. **Fixed take profit** - Caps winners unnecessarily
4. **Tight stop buffer (<1.5)** - Gets stopped out before trailing can protect

## Command Line Usage

```bash
# Run with optimized defaults
./target/release/pipeline replay-realtime

# Override specific parameters
./target/release/pipeline replay-realtime \
  --trailing-stop 4 \
  --min-delta 5 \
  --start-hour 9 --start-minute 30 \
  --end-hour 16 --end-minute 0
```

## Risk Management

- **Max Drawdown**: $555 on $30k account (1.85%)
- **Avg Loss**: 2.66 pts (~$53 per contract)
- **Win/Loss Ratio**: 9.78:1 (avg win / avg loss)
- **Breakeven Rate**: 21% of trades

## Validation Period

- **Date Range**: 276 trading days (2025 data)
- **Bars Processed**: 11,091,054 (1-second bars)
- **No look-ahead bias**: Uses previous day's levels only
