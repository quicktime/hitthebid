# Strategy Gaps Analysis - LVN Retest System

**Date:** 2025-12-29
**Status:** Critical gaps identified between stated CORE strategy and implementation

---

## The CORE Strategy (As Stated)

1. Trade ONLY 9:30 AM - 11:00 AM New York time
2. Pre-market: identify previous day high/low, VAH/VAL, POC, overnight high/low
3. Wait for market imbalance (range breakout) after open
4. Volume profile the impulse leg that created the breakout
5. Identify LVNs (Low Volume Nodes) in that impulse leg
6. Wait for pullback to the LVN
7. Look for LARGE delta aggression at the LVN (trapped traders covering)
8. Enter to continue the trend
9. After entry, RESET - remove those LVNs/profiles, wait for next breakout

---

## The Core Engine: Trapped Traders

The entire strategy is built on one phenomenon: **traders who entered at bad prices and will be forced to exit**.

When price moves impulsively through an LVN, one side gets trapped at bad prices. When price returns to that LVN, trapped traders exit (covering), creating order flow that continues the original move.

```
IMPULSE UP through LVN → Shorts trapped at LVN →
Price pulls back to LVN → Shorts cover (BUY) →
You BUY with them → Price continues UP
```

**Critical insight:** Trapped traders have a LIFESPAN. They don't stay trapped forever:
- First pullback = maximum trapped traders = HIGHEST EDGE
- After multiple retests = many already covered = REDUCED EDGE
- After you trade = they covered with you = EDGE IS GONE

---

## GAP 1: Trading Hours Default is WRONG

**Your Strategy:** Trade 9:30 AM - 11:00 AM NY time ONLY

**Current Implementation:** `src/pipeline/lvn_retest.rs:91-92`
```rust
trade_end_hour: 16,         // 4:00 PM ET (full RTH)
trade_end_minute: 0,
```

**Impact:** Backtest generates signals until 4:00 PM - nearly 5 extra hours of trades you don't take.

**Fix:** Change default to `trade_end_hour: 11`

---

## GAP 2: No Sequential "Breakout → Profile → Track" Flow

**Your Strategy:**
1. Wait for market imbalance (breakout) at open
2. Volume profile THAT impulse leg
3. Identify LVNs in THAT leg
4. Trade those LVNs on pullback
5. Reset and wait for NEXT breakout

**Current Implementation:** `src/pipeline/precompute.rs:57-59`
```rust
// Precomputes ALL impulse legs for the ENTIRE day at once
let impulse_legs = detect_impulse_legs(&bars_1m, &daily_levels);
let lvn_levels = extract_lvns(&trades, &impulse_legs);
```

**Problem:** The system batch-processes all impulses upfront. It doesn't:
- Wait for the FIRST breakout to establish direction
- Profile ONLY that specific impulse
- Reset after trading to wait for the NEXT breakout

The sequential, state-machine nature of your strategy is completely lost. You're trading LVNs from impulses that happen at 2:30 PM from data precomputed the night before.

---

## GAP 3: No True "Reset After Entry"

**Your Strategy:** "Once we enter, the LVNs we identified become irrelevant and we remove the volume profiles on the impulse legs, and reset."

**Current Implementation:** `src/pipeline/lvn_retest.rs:259-264`
```rust
// Only resets the SPECIFIC level traded
level.state = LevelState::Touched;
```

**Problem:** When you trade ONE LVN from an impulse leg, the OTHER LVNs from that same impulse remain active.

**Why this matters for trapped traders:** If trapped shorts covered at LVN #1 from an impulse, they're NOT going to cover again at LVN #2 from the same impulse - they're already out. The edge is consumed.

**Should:** Remove ALL LVNs from that impulse leg after a trade.

---

## GAP 4: No "Range Breakout" Validation

**Your Strategy:** "Wait until we see market imbalance... impulse leg that created the range breakout."

**Current Implementation:** `src/pipeline/impulse.rs:121-128`
```rust
let broke_swing = check_broke_swing(...); // Only checks 10-bar swing high/low
```

**Problem:** Impulse detection checks if it broke a 10-bar swing, but doesn't verify against key levels:
- Previous day high/low (PDH/PDL)
- Overnight high/low (ONH/ONL)
- Value Area High/Low (VAH/VAL)
- Opening Range

A "range breakout" means breaking a SIGNIFICANT level, not just a 10-bar swing.

---

## GAP 5: No "First Breakout of the Day" State Machine

**Your Strategy:** Implies waiting for the FIRST significant breakout after open, not any breakout.

**Current Implementation:** Treats all impulses throughout the day equally.

**Problem:** The 9:35 AM breakout that establishes the day's direction should be treated differently than a random 10:45 AM impulse. There's no state tracking for "have we had our first breakout yet?"

---

## GAP 6: `same_day_only: false` Default

**Your Strategy:** LVNs come from TODAY's impulse legs after the breakout.

**Current Implementation:** `src/pipeline/lvn_retest.rs:86`
```rust
same_day_only: false,       // Default: use all LVNs
```

Combined with `src/pipeline/replay_trading.rs:49`:
```rust
// Uses YESTERDAY's LVN levels
let yesterday = &days[i - 1];
trader.add_lvn_levels(&yesterday.lvn_levels);
```

**Problem:** You're trading off yesterday's LVNs (to avoid look-ahead bias), but your strategy is about TODAY's impulses. There's a fundamental mismatch.

---

## GAP 7: No Impulse Leg Grouping

**Current:** Each LVN is tracked independently with only `impulse_start_time` and `impulse_end_time`.

**Problem:** There's no unique impulse leg ID, so when you trade one LVN, you can't easily identify and invalidate all other LVNs from that same impulse leg.

---

## What the Current System Actually Does

```
CURRENT (BROKEN):
┌──────────────────────────────────────────────────────┐
│ Overnight: Precompute ALL impulses for day N        │
│            Extract ALL LVNs from all impulses       │
├──────────────────────────────────────────────────────┤
│ Day N+1:   Load yesterday's LVNs                    │
│            For each bar 9:30-16:00:                 │
│              - Is price at ANY LVN?                 │
│              - Is delta > 100?                      │
│              - Is market "imbalanced"?              │
│              → TRADE                                │
│              → Reset only that one level            │
└──────────────────────────────────────────────────────┘
```

---

## What the System SHOULD Do

```
CORRECT:
┌──────────────────────────────────────────────────────┐
│ Overnight: Precompute reference levels only         │
│            (PDH, PDL, POC, VAH, VAL, ONH, ONL)      │
├──────────────────────────────────────────────────────┤
│ Day N:                                               │
│                                                      │
│ STATE: WAITING_FOR_BREAKOUT                         │
│   │    Monitor price vs key levels                  │
│   │    (PDH/PDL, ONH/ONL, VAH/VAL)                 │
│   │                                                  │
│   ▼ Price breaks significant level                  │
│                                                      │
│ STATE: PROFILING_IMPULSE                            │
│   │    Track THIS impulse in real-time             │
│   │    Build volume profile as it forms            │
│   │    Extract LVNs when impulse completes         │
│   │    Assign impulse_group_id to all LVNs         │
│   │                                                  │
│   ▼ LVNs identified                                 │
│                                                      │
│ STATE: HUNTING                                       │
│   │    These LVNs = FRESH trapped traders          │
│   │    Wait for pullback to LVN                    │
│   │    Confirm with heavy delta (covering)         │
│   │                                                  │
│   ▼ Trade executed                                  │
│                                                      │
│ STATE: RESET                                         │
│   │    Trapped traders have covered                │
│   │    Clear ALL LVNs with this impulse_group_id   │
│   │                                                  │
│   └──► Return to WAITING_FOR_BREAKOUT              │
│                                                      │
│ 11:00 AM: STOP trading                              │
└──────────────────────────────────────────────────────┘
```

---

## Required State Machine

```rust
pub enum TradingState {
    /// Waiting for price to break a significant level
    WaitingForBreakout,

    /// Breakout detected, profiling the impulse leg
    ProfilingImpulse {
        breakout_level: f64,
        breakout_direction: Direction,
        impulse_start_bar: usize,
    },

    /// Impulse complete, hunting for LVN retest
    Hunting {
        impulse_group_id: u64,
        active_lvns: Vec<LvnLevel>,
    },

    /// In a trade, managing position
    InPosition {
        impulse_group_id: u64,
    },

    /// Trade complete, resetting for next cycle
    Resetting,
}
```

---

## Concrete Changes Needed

| Component | Current | Should Be |
|-----------|---------|-----------|
| Time filter default | 9:30-16:00 | 9:30-11:00 |
| Impulse detection | All day, batch | Real-time, on breakout only |
| LVN source | Yesterday's precompute | Current impulse only |
| Breakout validation | 10-bar swing | PDH/PDL, ONH/ONL, VAH/VAL |
| After trade | Reset single level | Clear ALL impulse LVNs |
| State tracking | None | Full state machine |
| Impulse grouping | None | Unique ID per impulse |

---

## Impact on Backtest Results

Current backtest results are likely:
1. **Overfitting** - trading signals that don't exist in your real strategy
2. **Including stale levels** - LVNs where trapped traders already exited
3. **Trading outside edge window** - 11:00 AM - 4:00 PM trades you'd never take
4. **Double-counting** - trading multiple LVNs from same impulse (same trapped traders)
5. **Using wrong LVNs** - yesterday's levels instead of today's fresh impulse levels

The positive results don't reflect the edge of your actual strategy.

---

## Files That Need Changes

1. `src/pipeline/lvn_retest.rs` - Add state machine, fix defaults
2. `src/pipeline/impulse.rs` - Add breakout level validation
3. `src/pipeline/lvn.rs` - Add impulse_group_id
4. `src/pipeline/replay_trading.rs` - Use real-time detection, not precompute
5. `src/pipeline/rithmic_live.rs` - Integrate state machine
6. `src/pipeline/precompute.rs` - Only precompute reference levels, not signals

---

## Priority Order

1. **QUICK FIX:** Change time filter default to 11:00 AM
2. **MEDIUM:** Add impulse_group_id and reset all LVNs from same impulse after trade
3. **MAJOR:** Implement state machine for real-time breakout detection
4. **MAJOR:** Add breakout level validation (PDH/PDL, ONH/ONL, VAH/VAL)
5. **REFACTOR:** Change replay to use real-time detection instead of precompute
