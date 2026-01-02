# State Machine Implementation Plan

**Created:** 2025-12-29
**Status:** Approved, ready for implementation

---

## Goal

Implement a real-time trading state machine that follows the CORE strategy:

1. **Wait for breakout** - Price breaks PDH/PDL, ONH/ONL, VAH/VAL
2. **Profile impulse** - Track the impulse leg in real-time as it forms
3. **Extract LVNs** - Get LVNs from THAT specific impulse
4. **Hunt for retest** - Wait for pullback to LVN with delta confirmation
5. **Trade and RESET** - After trade, clear ALL LVNs from that impulse (trapped traders covered)
6. **Repeat** - Return to waiting for next breakout

---

## State Machine States

```
WAITING_FOR_BREAKOUT
    │ Price breaks significant level (PDH/PDL/ONH/ONL/VAH/VAL)
    ▼
PROFILING_IMPULSE
    │ Impulse completes (fast move, volume, score >= 4)
    ▼
HUNTING
    │ Trade executed OR timeout
    ▼
RESET → Back to WAITING_FOR_BREAKOUT
```

---

## User Decisions

1. **11:00 AM Cutoff**: Stop generating NEW signals at 11:00, but let open positions run to stop/target
2. **Backward Compatibility**: Keep both `ReplayTest` (old) and `ReplayRealtime` (new) commands
3. **Impulse Timeout**: If impulse doesn't complete within timeout, RESET and wait for next breakout (don't use partial LVNs)

---

## Implementation Phases

### Phase 1: Core Data Structures

**Files:** `Cargo.toml`, `src/pipeline/impulse.rs`, `src/pipeline/lvn.rs`

1. ✅ `uuid` already in Cargo.toml with v4 and serde features

2. Add `id: Uuid` to `ImpulseLeg` in `impulse.rs`:
```rust
pub struct ImpulseLeg {
    pub id: Uuid,  // NEW: Unique identifier
    // ... existing fields
}
```

3. Add `impulse_id: Uuid` to `LvnLevel` in `lvn.rs`:
```rust
pub struct LvnLevel {
    pub impulse_id: Uuid,  // NEW: Links to parent impulse
    // ... existing fields
}
```

4. Add `RealTimeImpulseBuilder` to `impulse.rs`:
```rust
pub struct RealTimeImpulseBuilder {
    id: Uuid,
    start_time: DateTime<Utc>,
    start_price: f64,
    direction: ImpulseDirection,
    bars: Vec<Bar>,
    high: f64,
    low: f64,
}

impl RealTimeImpulseBuilder {
    pub fn new(start_bar: &Bar, direction: ImpulseDirection) -> Self;
    pub fn add_bar(&mut self, bar: &Bar);
    pub fn is_complete(&self) -> bool;
    pub fn score(&self) -> u8;
    pub fn finalize(self) -> ImpulseLeg;
}
```

5. Add `extract_lvns_realtime()` to `lvn.rs`:
```rust
pub fn extract_lvns_realtime(
    trades: &[Trade],
    impulse_id: Uuid,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    direction: ImpulseDirection,
    symbol: &str,
) -> Vec<LvnLevel>;
```

### Phase 2: State Machine

**Files:** NEW `src/pipeline/state_machine.rs`

Create the core state machine module:

```rust
pub enum TradingState {
    WaitingForBreakout,
    ProfilingImpulse,
    Hunting,
    Reset,
}

pub enum BreakoutLevel {
    PDH, PDL, ONH, ONL, VAH, VAL,
}

pub struct ActiveImpulse {
    pub id: Uuid,
    pub direction: ImpulseDirection,
    pub broken_level: BreakoutLevel,
    pub bars: Vec<Bar>,
    pub trades: Vec<Trade>,
    pub start_time: DateTime<Utc>,
    pub start_price: f64,
    pub high: f64,
    pub low: f64,
}

pub struct StateMachineConfig {
    pub breakout_threshold: f64,      // 2.0 points
    pub max_impulse_bars: usize,      // 300 (5 min)
    pub min_impulse_size: f64,        // 30.0 points
    pub max_hunting_bars: usize,      // 600 (10 min)
    pub min_impulse_score: u8,        // 4/5
}

pub struct TradingStateMachine {
    config: StateMachineConfig,
    state: TradingState,
    daily_levels: Option<LiveDailyLevels>,
    active_impulse: Option<ActiveImpulse>,
    active_lvns: Vec<LvnLevel>,
    hunting_start_bar: Option<usize>,
    bar_count: usize,
}

impl TradingStateMachine {
    pub fn new(config: StateMachineConfig) -> Self;
    pub fn set_daily_levels(&mut self, levels: LiveDailyLevels);
    pub fn process_bar(&mut self, bar: &Bar) -> Option<StateTransition>;
    pub fn process_trade(&mut self, trade: &Trade);
    pub fn state(&self) -> TradingState;
    pub fn active_lvns(&self) -> &[LvnLevel];
    pub fn clear_impulse_lvns(&mut self);
    pub fn reset(&mut self);

    fn check_breakout(&self, bar: &Bar) -> Option<(BreakoutLevel, ImpulseDirection)>;
    fn start_impulse(&mut self, bar: &Bar, level: BreakoutLevel, direction: ImpulseDirection);
    fn update_impulse(&mut self, bar: &Bar) -> bool;
    fn check_impulse_completion(&self) -> bool;
    fn extract_lvns_from_impulse(&mut self) -> Vec<LvnLevel>;
    fn score_impulse(&self) -> u8;
}

pub enum StateTransition {
    BreakoutDetected { level: BreakoutLevel, direction: ImpulseDirection },
    ImpulseComplete { lvn_count: usize },
    ImpulseInvalid { reason: String },
    HuntingTimeout,
    Reset,
}
```

### Phase 3: Level Tracking

**Files:** `src/pipeline/levels.rs`

Add live level tracking:

```rust
pub struct LiveDailyLevels {
    pub date: NaiveDate,
    pub pdh: f64,
    pub pdl: f64,
    pub onh: f64,
    pub onl: f64,
    pub vah: f64,
    pub val: f64,
    pub session_high: f64,
    pub session_low: f64,
}

impl LiveDailyLevels {
    pub fn from_daily_levels(levels: &DailyLevels) -> Self;
    pub fn check_breakout(&self, price: f64, threshold: f64) -> Option<(BreakoutLevel, ImpulseDirection)>;
}
```

### Phase 4: Signal Generator Updates

**Files:** `src/pipeline/lvn_retest.rs`

Add impulse-aware level management:

```rust
// Modify TrackedLevel
pub struct TrackedLevel {
    pub impulse_id: Option<Uuid>,  // NEW
    // ... existing fields
}

impl LvnSignalGenerator {
    // NEW methods
    pub fn add_lvn_levels_with_impulse(&mut self, levels: &[LvnLevel], impulse_id: Uuid);
    pub fn clear_impulse_lvns(&mut self, impulse_id: Uuid);
    pub fn get_level_impulse_id(&self, level_key: i64) -> Option<Uuid>;
}
```

### Phase 5: LiveTrader Integration

**Files:** `src/pipeline/rithmic_live.rs`

Integrate state machine:

```rust
pub struct LiveTrader {
    state_machine: Option<TradingStateMachine>,  // NEW (Option for backward compat)
    use_state_machine: bool,  // NEW flag
    // ... existing fields
}

impl LiveTrader {
    pub fn new_with_state_machine(config: LiveConfig, sm_config: StateMachineConfig) -> Self;

    pub fn process_bar(&mut self, bar: &Bar) -> Option<TradeAction> {
        if self.use_state_machine {
            // Use state machine flow
            if let Some(ref mut sm) = self.state_machine {
                if let Some(transition) = sm.process_bar(bar) {
                    match transition {
                        StateTransition::ImpulseComplete { .. } => {
                            let lvns = sm.active_lvns();
                            let impulse_id = lvns.first().map(|l| l.impulse_id);
                            if let Some(id) = impulse_id {
                                self.signal_gen.add_lvn_levels_with_impulse(lvns, id);
                            }
                        }
                        // ... handle other transitions
                    }
                }
            }
        }

        // ... existing logic for position management and signals

        // After trade exit, clear impulse LVNs
        if trade_exited && self.use_state_machine {
            if let Some(impulse_id) = self.signal_gen.get_level_impulse_id(level_key) {
                self.signal_gen.clear_impulse_lvns(impulse_id);
                if let Some(ref mut sm) = self.state_machine {
                    sm.reset();
                }
            }
        }
    }
}
```

### Phase 6: Replay Testing

**Files:** `src/pipeline/replay_trading.rs`, `src/pipeline/main.rs`

1. Add new replay function:

```rust
pub async fn run_replay_realtime(
    cache_dir: PathBuf,
    date: Option<String>,
    config: LiveConfig,
    sm_config: StateMachineConfig,
) -> Result<TradingSummary> {
    let mut trader = LiveTrader::new_with_state_machine(config, sm_config);

    let days = precompute::load_all_cached(&cache_dir, date.as_deref())?;

    for (i, day) in days.iter().enumerate() {
        // Load daily levels (not LVNs - those come from state machine)
        if !day.daily_levels.is_empty() {
            let levels = LiveDailyLevels::from_daily_levels(&day.daily_levels[0]);
            trader.set_daily_levels(levels);
        }

        for bar in &day.bars_1s {
            let _ = trader.process_bar(bar);
        }

        trader.reset_for_new_day(last_price);
    }

    Ok(trader.summary())
}
```

2. Add CLI command in `main.rs`:

```rust
/// Replay test with REAL-TIME state machine (no precomputed LVNs)
ReplayRealtime {
    #[arg(short, long, default_value = "cache_2025")]
    cache_dir: PathBuf,

    #[arg(short = 'D', long)]
    date: Option<String>,

    // Existing trading params...

    #[arg(long, default_value = "2.0")]
    breakout_threshold: f64,

    #[arg(long, default_value = "300")]
    max_impulse_bars: usize,

    #[arg(long, default_value = "600")]
    max_hunting_bars: usize,
}
```

---

## Default Configuration

```rust
StateMachineConfig {
    breakout_threshold: 2.0,      // Points beyond level to confirm breakout
    max_impulse_bars: 300,        // 5 min timeout for impulse profiling (1s bars)
    min_impulse_size: 30.0,       // 30 points minimum (from existing impulse.rs)
    max_hunting_bars: 600,        // 10 min timeout for hunting
    min_impulse_score: 4,         // 4/5 criteria (from existing impulse.rs)
}
```

---

## Key Design Decisions

1. **Separate state_machine.rs module** - Clean separation, easier testing
2. **UUID for impulse grouping** - Allows bulk LVN clearing after trade
3. **Real-time volume profile from trades** - More accurate than bar-based
4. **Hunting timeout** - Prevents stale LVNs, allows new breakout cycle
5. **Keep old replay mode** - Can compare results between approaches
6. **Optional state machine in LiveTrader** - Backward compatible, can toggle on/off

---

## Files Summary

| File | Action | Description |
|------|--------|-------------|
| `src/pipeline/state_machine.rs` | CREATE | Core state machine |
| `src/pipeline/impulse.rs` | MODIFY | Add Uuid, RealTimeImpulseBuilder |
| `src/pipeline/lvn.rs` | MODIFY | Add impulse_id, extract_lvns_realtime |
| `src/pipeline/levels.rs` | MODIFY | Add LiveDailyLevels |
| `src/pipeline/lvn_retest.rs` | MODIFY | Add impulse methods to signal generator |
| `src/pipeline/rithmic_live.rs` | MODIFY | Integrate state machine |
| `src/pipeline/replay_trading.rs` | MODIFY | Add run_replay_realtime |
| `src/pipeline/main.rs` | MODIFY | Add ReplayRealtime command |
| `src/pipeline/mod.rs` | MODIFY | Export state_machine module |

---

## Testing Strategy

1. Unit test state transitions in isolation
2. Unit test breakout detection for each level type (PDH, PDL, ONH, ONL, VAH, VAL)
3. Integration test: run `ReplayRealtime` vs old `ReplayTest` on same data
4. Compare trade counts, win rates, P&L between modes
5. Log all state transitions for debugging

---

## Related Documents

- `STRATEGY_GAPS_ANALYSIS.md` - Original gap analysis
- `CLAUDE.md` - Project overview
