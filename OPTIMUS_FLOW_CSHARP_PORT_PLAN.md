# Optimus Flow C# Port - Implementation Plan

## Decisions

- **Location**: `optimus_flow_strategy/` in repo root
- **Scope**: Full port (all features - state machine, LVN extraction, signal generation)
- **Cleanup**: Delete obsolete bridge code (`src/optimus/`, `optimus_executor/`)

## Overview

Port the autonomous LVN trading strategy from Rust to C# to run natively in Optimus Flow (Quantower). This eliminates the Rust→C# bridge latency and provides a single-process solution.

## What Gets Ported vs What Stays

| Component | Port to C#? | Notes |
|-----------|-------------|-------|
| Bar Aggregation | YES | 1-second bars with delta from Databento ticks |
| Daily Levels | YES | PDH/PDL/VAH/VAL computation |
| State Machine | YES | Breakout → Impulse → Hunting flow |
| LVN Extraction | YES | Volume profile analysis |
| Signal Generator | YES | Level tracking and signal detection |
| Position Management | YES | Trailing stops, P&L, risk limits |
| Execution | YES | Optimus Flow/Quantower API |
| Frontend (React) | NO | Keep in Rust repo |
| Backtesting | NO | Keep in Rust repo |
| WebSocket Server | NO | Not needed |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Optimus Flow (C#)                        │
│                                                             │
│  Databento.Client ──► BarAggregator ──► LiveTrader         │
│       │                    │               │                │
│       │                    ▼               ▼                │
│       │              StateMachine ◄── SignalGenerator       │
│       │                    │               │                │
│       │                    ▼               │                │
│       │              LvnExtractor ─────────┘                │
│       │                                                     │
│       └──────────────────────────────────────► Quantower    │
│                                                 Order API   │
└─────────────────────────────────────────────────────────────┘
```

## Project Structure

```
optimus_flow_strategy/
├── LvnStrategy.sln
├── LvnStrategy/
│   ├── LvnStrategy.csproj
│   │
│   ├── Models/
│   │   ├── Bar.cs                 # OHLCV + delta
│   │   ├── Trade.cs               # Raw tick data
│   │   ├── LvnLevel.cs            # Low volume node
│   │   ├── DailyLevels.cs         # PDH/PDL/VAH/VAL
│   │   ├── OpenPosition.cs        # Active position state
│   │   └── TradeAction.cs         # Entry/Exit/UpdateStop/Flatten
│   │
│   ├── Core/
│   │   ├── BarAggregator.cs       # Tick → 1-second bars
│   │   ├── DailyLevelsCalculator.cs
│   │   ├── StateMachine.cs        # 4-state trading state machine
│   │   ├── ImpulseBuilder.cs      # Real-time impulse tracking
│   │   ├── LvnExtractor.cs        # Volume profile → LVN extraction
│   │   ├── SignalGenerator.cs     # Level tracking + signal detection
│   │   ├── MarketStateDetector.cs # Balanced vs Imbalanced
│   │   └── LiveTrader.cs          # Position management + P&L
│   │
│   ├── Config/
│   │   ├── TradingConfig.cs       # All tunable parameters
│   │   └── Symbols.cs             # Contract mappings
│   │
│   ├── Data/
│   │   ├── DatabentoClient.cs     # Databento C# SDK wrapper
│   │   └── LevelCache.cs          # Load/save daily levels
│   │
│   ├── Execution/
│   │   └── QuantowerExecutor.cs   # Optimus Flow order API
│   │
│   └── Strategy/
│       └── LvnTradingStrategy.cs  # Main Quantower strategy class
│
└── README.md
```

## Phase 1: Foundation (Models + Config)

### Files to Create:

**1. Models/Bar.cs**
```csharp
public class Bar
{
    public DateTime Timestamp { get; set; }
    public double Open { get; set; }
    public double High { get; set; }
    public double Low { get; set; }
    public double Close { get; set; }
    public ulong Volume { get; set; }
    public ulong BuyVolume { get; set; }
    public ulong SellVolume { get; set; }
    public long Delta => (long)BuyVolume - (long)SellVolume;
    public ulong TradeCount { get; set; }
    public string Symbol { get; set; }

    public bool IsBullish => Close > Open;
    public double BodySize => Math.Abs(Close - Open);
    public double Range => High - Low;
}
```

**2. Models/DailyLevels.cs** - PDH, PDL, ONH, ONL, VAH, VAL, POC

**3. Models/LvnLevel.cs** - Price, volume ratio, impulse direction, date

**4. Models/OpenPosition.cs** - Direction, entry, stops, P&L tracking

**5. Models/TradeAction.cs** - Abstract base with Entry/Exit/UpdateStop/Flatten subclasses

**6. Config/TradingConfig.cs** - All parameters from Rust LiveConfig + LvnRetestConfig

## Phase 2: Data Layer

### Files to Create:

**1. Core/BarAggregator.cs**
- Port from `databento_ib_live.rs:36-135`
- Key: Group trades by second, track buy/sell volume separately
- Delta = BuyVolume - SellVolume

**2. Data/DatabentoClient.cs**
- Use `Databento.Client` NuGet package
- Subscribe to GLBX.MDP3 trades schema
- Parse price: `trade.Price / 1_000_000_000.0`
- Parse side: 'A' = Buy, 'B' = Sell

**3. Core/DailyLevelsCalculator.cs**
- Port from `daily_levels.rs`
- Volume profile with $1.0 buckets
- POC = bucket with max volume
- Value Area = expand from POC until 70% volume captured

**4. Data/LevelCache.cs**
- JSON serialization of daily levels
- Load on startup, save after RTH close

## Phase 3: Trading Logic

### Files to Create:

**1. Core/StateMachine.cs**
- Port from `state_machine.rs`
- States: WaitingForBreakout, ProfilingImpulse, Hunting, Reset
- Key config: breakout_threshold=2.0, min_impulse_size=25.0, max_impulse_bars=300

**2. Core/ImpulseBuilder.cs**
- Port from `impulse.rs:RealTimeImpulseBuilder`
- Track bars during impulse
- Score: broke_swing + fast + uniform + volume_increased + sufficient_size
- Minimum score: 4/5

**3. Core/LvnExtractor.cs**
- Port from `lvn.rs:extract_lvns_realtime`
- Build volume profile from trades (0.5-point buckets)
- LVN = price level with volume < 15% of average

**4. Core/MarketStateDetector.cs**
- Port from `market_state.rs`
- 60-bar lookback window
- Imbalanced if: range > 2×ATR OR |cumulative_delta| > 200

**5. Core/SignalGenerator.cs**
- Port from `lvn_retest.rs:LvnSignalGenerator`
- Level states: Untouched → Touched → Armed → Retesting
- Signal when: Market Imbalanced + At Level + Heavy Delta in trend direction

**6. Core/LiveTrader.cs**
- Port from `trader.rs:LiveTrader`
- Position management with trailing stops
- Daily loss limits
- P&L calculation with slippage/commission

## Phase 4: Execution

### Files to Create:

**1. Execution/QuantowerExecutor.cs**
```csharp
public class QuantowerExecutor
{
    private Symbol _symbol;
    private Account _account;

    public async Task ExecuteAsync(TradeAction action)
    {
        switch (action)
        {
            case TradeAction.Enter enter:
                // Market order + Stop + Target bracket
                Core.Instance.PlaceOrder(_symbol, _account,
                    enter.Direction == Direction.Long ? Side.Buy : Side.Sell,
                    quantity: enter.Contracts);
                // Place stop order
                // Place target order
                break;

            case TradeAction.Exit exit:
                // Close position
                Core.Instance.ClosePosition(position);
                break;

            case TradeAction.UpdateStop update:
                // Modify stop order
                Core.Instance.ModifyOrder(stopOrder, triggerPrice: update.NewStop);
                break;

            case TradeAction.FlattenAll flatten:
                // Cancel all orders + close position
                break;
        }
    }
}
```

**2. Strategy/LvnTradingStrategy.cs**
- Inherits from Quantower Strategy base class
- OnRun(): Initialize Databento client, start trading loop
- OnStop(): Flatten positions, cleanup
- Main loop: Databento ticks → Bars → Trader → Executor

## Phase 5: Integration & Testing

1. **Paper Trading**
   - Connect to Databento live feed
   - Connect to Optimus Flow demo account
   - Run for full RTH session
   - Verify signals match Rust output

2. **Position Sync**
   - Sync local state with broker position
   - Handle fills and partial fills
   - Handle disconnections gracefully

3. **Logging**
   - Trade log CSV (matches Rust format)
   - Console output for debugging
   - Signal timestamps for latency analysis

## Critical Parameters to Match

| Parameter | Value | Source File |
|-----------|-------|-------------|
| breakout_threshold | 2.0 pts | state_machine.rs |
| min_impulse_size | 25.0 pts | state_machine.rs |
| min_impulse_score | 4 | state_machine.rs |
| max_impulse_bars | 300 (5 min) | state_machine.rs |
| max_hunting_bars | 600 (10 min) | state_machine.rs |
| max_retrace_ratio | 0.7 | state_machine.rs |
| LVN_BUCKET_SIZE | 0.5 pts | lvn.rs |
| LVN_THRESHOLD_RATIO | 0.15 | lvn.rs |
| level_tolerance | 2.0 pts | lvn_retest.rs |
| retest_distance | 8.0 pts | lvn_retest.rs |
| min_delta_for_absorption | 100 | lvn_retest.rs |
| max_range_for_absorption | 1.5 pts | lvn_retest.rs |
| trailing_stop | 4.0 pts | config |
| stop_buffer | 2.0 pts | config |
| cooldown_bars | 60 (1 min) | lvn_retest.rs |
| level_cooldown_bars | 600 (10 min) | lvn_retest.rs |

## Dependencies

```xml
<ItemGroup>
  <!-- Databento C# client -->
  <PackageReference Include="Databento.Client" Version="5.1.4" />

  <!-- JSON serialization -->
  <PackageReference Include="System.Text.Json" Version="8.0.0" />

  <!-- Quantower SDK (from Optimus Flow installation) -->
  <Reference Include="TradingPlatform.BusinessLayer">
    <HintPath>C:\Program Files\Optimus Flow\TradingPlatform.BusinessLayer.dll</HintPath>
  </Reference>
</ItemGroup>
```

## Environment Variables

```
DATABENTO_API_KEY=your_key
```

## Success Criteria

1. Signals match Rust output on same data feed
2. Trailing stop updates correctly
3. Daily loss limits enforced
4. Positions sync with broker
5. < 50ms latency from signal to order submission
6. Survives full RTH session without crashes

## Estimated Effort

| Phase | Effort |
|-------|--------|
| Phase 1: Models | 2-3 hours |
| Phase 2: Data Layer | 4-6 hours |
| Phase 3: Trading Logic | 8-12 hours |
| Phase 4: Execution | 4-6 hours |
| Phase 5: Testing | 4-8 hours |
| **Total** | **22-35 hours** |

## Files to Reference During Port

| C# Component | Rust Source |
|--------------|-------------|
| BarAggregator | `src/pipeline/databento_ib_live.rs:36-135` |
| DailyLevelsCalculator | `src/trading_core/daily_levels.rs` |
| StateMachine | `src/trading_core/state_machine.rs` |
| ImpulseBuilder | `src/trading_core/impulse.rs` |
| LvnExtractor | `src/trading_core/lvn.rs` |
| SignalGenerator | `src/trading_core/lvn_retest.rs` |
| LiveTrader | `src/trading_core/trader.rs` |
| Bar struct | `src/trading_core/bars.rs` |
