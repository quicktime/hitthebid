# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Hit The Bid** - Real-time orderflow visualization and LVN trading signals for NQ/ES futures. Displays trade aggression bubbles, CVD (Cumulative Volume Delta), absorption detection, stacked imbalances, confluence signals, and automated LVN retest trading strategy.

## Build & Run Commands

```bash
# Backend (Rust)
cargo build --release                            # Build release binary
./target/release/hitthebid                       # Live mode (requires DATABENTO_API_KEY)
./target/release/hitthebid --demo                # Demo mode with simulated data
./target/release/hitthebid --symbols NQH6 --trading  # Live trading mode with LVN signals

# Frontend (React/Vite)
npm run dev                                      # Dev server (port 5173)
npm run build                                    # Production build to dist/
npm run lint                                     # ESLint

# Combined development workflow
npm run build && ./target/release/hitthebid --demo   # Build frontend, run backend on port 8080
```

## Trading Mode

When `--trading` flag is enabled:
1. **Daily Levels Fetch**: On startup, fetches yesterday's data from Databento historical API
2. **Level Computation**: Computes PDH, PDL, POC, VAH, VAL, ONH, ONL from RTH/overnight sessions
3. **State Machine**: Detects breakouts → profiles impulse → extracts LVNs → hunts for retests
4. **Trade Logging**: All entries/exits/stops logged to `trades.csv`

Key trading files:
- `src/trading_core/daily_levels.rs` - Fetches fresh levels from Databento historical API
- `src/trading_core/state_machine.rs` - Trading state machine (WaitingForBreakout → ProfilingImpulse → Hunting)
- `src/trading_core/trader.rs` - LiveTrader with position management
- `src/streams/live.rs` - Real-time data processing with CSV trade logging

## Architecture

**Rust Backend** (`src/*.rs`):
- `main.rs` - Axum web server, WebSocket handler, CLI args (clap)
- `types.rs` - Shared data structures (Trade, Bubble, CVDPoint, AbsorptionZone, WsMessage enum)
- `processing.rs` - ProcessingState: trade aggregation, CVD calculation, absorption detection
- `trading_core/` - LVN trading strategy modules
- `streams/` - Data sources:
  - `live.rs` - Databento real-time feed with trading integration
  - `demo.rs` - Simulated data for testing
  - `replay.rs` - Historical replay from Databento batch API

**React Frontend** (`src/*.tsx`):
- `App.tsx` - Main component, WebSocket client, state management, keyboard shortcuts, audio alerts
- `BubbleRenderer.tsx` - Canvas rendering for bubbles, CVD line, volume profile, absorption zones
- `websocket.ts` - RustWebSocket class for backend connection

**Data Flow**: Databento -> Rust (parse trades) -> ProcessingState + LiveTrader -> WebSocket (JSON) -> React (canvas render)

## Key Concepts

- **Bubble**: Aggregated trades over short window showing dominant side (buy/sell) and imbalance
- **CVD**: Running delta of buy vs sell volume; zero-crosses are trading signals
- **Absorption**: High delta with minimal price movement indicates institutional defense
- **Stacked Imbalances**: 3+ consecutive price levels with 70%+ one-side dominance
- **LVN (Low Volume Node)**: Price level with minimal volume in impulse profile - key retest target
- **Daily Levels**: PDH/PDL (Prior Day High/Low), POC (Point of Control), VAH/VAL (Value Area), ONH/ONL (Overnight)

## WebSocket Message Types

All messages use tagged JSON with `type` field. See `types.rs` WsMessage enum for variants:
`Bubble`, `CVDPoint`, `VolumeProfile`, `Absorption`, `AbsorptionZones`, `DeltaFlip`, `StackedImbalance`, `Confluence`, `SessionStats`, `TradingSignal`, `Connected`, `Error`

## Environment

Requires `DATABENTO_API_KEY` in `.env` for live/replay modes. Demo mode works without credentials.

## Current Symbol

Default symbol is `NQH6` (NQ March 2026 contract). Change with `--symbols` flag.
