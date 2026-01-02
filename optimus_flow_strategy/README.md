# LVN Trading Strategy for Optimus Flow

A C# port of the autonomous LVN (Low Volume Node) trading strategy for native execution in Optimus Flow (Quantower).

## Overview

This strategy implements the Fabio Valentini Trend Model:
1. **Wait for Breakout** - Detect when price breaks PDH/PDL, ONH/ONL, or VAH/VAL
2. **Profile Impulse** - Track the impulse leg and build volume profile
3. **Extract LVNs** - Find low volume nodes (thin price levels)
4. **Hunt for Retest** - Wait for price to return to LVN with absorption
5. **Execute Trade** - Enter with trailing stop when absorption detected

## Architecture

```
Databento.Client ──► BarAggregator ──► LiveTrader
     │                    │               │
     │                    ▼               ▼
     │              StateMachine ◄── SignalGenerator
     │                    │               │
     │                    ▼               │
     │              LvnExtractor ─────────┘
     │
     └──────────────────────────────────► QuantowerExecutor
                                              │
                                              ▼
                                         Quantower API
```

## Project Structure

```
LvnStrategy/
├── Models/           # Data types (Bar, Trade, LvnLevel, etc.)
├── Core/             # Trading logic
│   ├── BarAggregator.cs       # Tick → 1-second bars
│   ├── StateMachine.cs        # 4-state trading state machine
│   ├── ImpulseBuilder.cs      # Impulse scoring
│   ├── LvnExtractor.cs        # Volume profile → LVN extraction
│   ├── SignalGenerator.cs     # Level tracking + signal detection
│   ├── MarketStateDetector.cs # Balanced vs Imbalanced
│   └── LiveTrader.cs          # Position management + P&L
├── Config/           # Trading configuration
├── Data/             # Data access (Databento, cache)
├── Execution/        # Order execution (Quantower)
└── Strategy/         # Main strategy entry point
```

## Prerequisites

1. **Visual Studio 2022** (Community edition is free)
2. **.NET 8 SDK**
3. **Optimus Flow** from [Optimus Futures](https://optimusfutures.com/OptimusFlow.php)
4. **Databento API key** for live market data

## Setup

### 1. Install Dependencies

Open the solution in Visual Studio and restore NuGet packages:
- `Databento.Client` (5.1.4) - Live market data
- `System.Text.Json` (8.0.0) - JSON serialization

### 2. Add Quantower SDK Reference

Uncomment the reference in `LvnStrategy.csproj` and update the path:

```xml
<Reference Include="TradingPlatform.BusinessLayer">
  <HintPath>C:\Program Files\Optimus Flow\TradingPlatform.BusinessLayer.dll</HintPath>
</Reference>
```

### 3. Set Environment Variable

```bash
set DATABENTO_API_KEY=your_api_key_here
```

### 4. Enable Quantower API Calls

In the following files, uncomment the Quantower API code:
- `Execution/QuantowerExecutor.cs`
- `Data/DatabentoClient.cs`

## Usage in Optimus Flow

1. Build the solution to create `LvnStrategy.dll`
2. Open Optimus Flow → Algo → Algo Lab
3. Create a new Strategy project
4. Add reference to `LvnStrategy.dll`
5. Implement a wrapper strategy:

```csharp
public class MyLvnStrategy : Strategy
{
    private LvnTradingStrategy? _strategy;

    protected override void OnRun()
    {
        var config = TradingConfig.DefaultMnq();
        _strategy = new LvnTradingStrategy(config);
        _strategy.OnLog += (_, msg) => Log(msg);
        Task.Run(() => _strategy.StartAsync("MNQH6"));
    }

    protected override void OnStop()
    {
        _strategy?.StopAsync().GetAwaiter().GetResult();
    }
}
```

## Configuration

Key parameters in `TradingConfig`:

| Parameter | Default | Description |
|-----------|---------|-------------|
| Contracts | 1 | Number of contracts to trade |
| TrailingStop | 4.0 | Trailing stop distance in points |
| StopBuffer | 2.0 | Buffer beyond LVN for stop placement |
| MinDelta | 150 | Minimum delta for absorption signal |
| BreakoutThreshold | 2.0 | Points beyond level for breakout |
| MinImpulseSize | 25.0 | Minimum impulse size in points |
| MinImpulseScore | 4 | Minimum score (out of 5) for valid impulse |
| LvnThresholdRatio | 0.15 | Volume ratio for LVN qualification |
| MaxDailyLosses | 3 | Maximum losing trades per day |

## Trading Hours

Default: 9:30 AM - 4:00 PM Eastern Time

The strategy will:
- Only enter new positions during trading hours
- Flatten all positions at end of day
- Stop trading after daily loss limits are hit

## Cost Structure

With Optimus Futures ($500 minimum account):
- MNQ commission: $0.25/side ($0.50 round-trip)
- Platform: FREE
- CME data: ~$5/month

## License

This code is provided for educational purposes. Use at your own risk.
