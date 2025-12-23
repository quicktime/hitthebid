# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Real-time orderflow visualization for NQ/ES futures. Displays trade aggression bubbles, CVD (Cumulative Volume Delta), absorption detection, stacked imbalances, and confluence signals.

## Build & Run Commands

```bash
# Backend (Rust)
cargo run --release                              # Live mode (requires DATABENTO_API_KEY)
cargo run --release -- --demo                    # Demo mode with simulated data
cargo run --release -- --replay --replay-date 2024-12-20  # Historical replay

# Frontend (React/Vite)
npm run dev                                      # Dev server (port 5173)
npm run build                                    # Production build to dist/
npm run lint                                     # ESLint

# Combined development workflow
npm run build && cargo run --release -- --demo   # Build frontend, run backend on port 8080
```

## Architecture

**Rust Backend** (`src/*.rs`):
- `main.rs` - Axum web server, WebSocket handler, CLI args (clap)
- `types.rs` - Shared data structures (Trade, Bubble, CVDPoint, AbsorptionZone, WsMessage enum)
- `processing.rs` - ProcessingState: trade aggregation, CVD calculation, absorption detection, stacked imbalances, confluence detection, volume profile
- `streams/` - Data sources:
  - `live.rs` - Databento real-time feed
  - `demo.rs` - Simulated data for testing
  - `replay.rs` - Historical replay from Databento batch API

**React Frontend** (`src/*.tsx`):
- `App.tsx` - Main component, WebSocket client, state management, keyboard shortcuts, audio alerts
- `BubbleRenderer.tsx` - Canvas rendering for bubbles, CVD line, volume profile, absorption zones
- `websocket.ts` - RustWebSocket class for backend connection

**Data Flow**: Databento -> Rust (parse MBO trades) -> ProcessingState (aggregate/analyze) -> WebSocket (WsMessage JSON) -> React (canvas render)

## Key Concepts

- **Bubble**: Aggregated trades over short window showing dominant side (buy/sell) and imbalance
- **CVD**: Running delta of buy vs sell volume; zero-crosses are trading signals
- **Absorption**: High delta with minimal price movement indicates institutional defense
- **Stacked Imbalances**: 3+ consecutive price levels with 70%+ one-side dominance
- **Confluence**: Multiple signals (delta flip, absorption, stacked) within 5 seconds

## WebSocket Message Types

All messages use tagged JSON with `type` field. See `types.rs` WsMessage enum for variants:
`Bubble`, `CVDPoint`, `VolumeProfile`, `Absorption`, `AbsorptionZones`, `DeltaFlip`, `StackedImbalance`, `Confluence`, `SessionStats`, `Connected`, `Error`

## Environment

Requires `DATABENTO_API_KEY` in `.env` for live/replay modes. Demo mode works without credentials.
