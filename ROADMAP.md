# FLOW - Orderflow Bubbles Development Roadmap

> **Strategy:** Fabio Valentini's Nasdaq Orderflow Scalping
>
> **Goal:** Build a professional orderflow visualization tool for futures trading

---

## ✅ PHASE 1: COMPLETE - Core Orderflow Signals

### **Implemented Features:**

#### 1. **Real-Time Bubbles Visualization**
- Buy/sell aggression bubbles (green/red)
- Size-based scaling (larger trades = bigger bubbles)
- 2.5-minute history (~1,000 bubbles max)
- Price grid with automatic scaling
- Smooth 60fps animation

#### 2. **Enhanced Large Trade Visuals** (Fabio's Tiers)
- **10-49 contracts:** Standard glow
- **50-99 contracts:** Enhanced glow (medium institutional)
- **100-199 contracts:** Strong glow (large institutional)
- **200+ contracts:** Massive glow (major player/absorption)

#### 3. **Stacked Imbalance Detection**
- Detects 3+ consecutive same-side trades within 5 points
- Gold border (3px thick) for stacked bubbles
- Gold glow overlay
- Marks previous 2 bubbles when pattern confirmed
- **Signal:** Trapped traders revealing themselves

#### 4. **CVD (Cumulative Volume Delta)**
- Bottom panel line chart (80px height)
- Shows running total of buy - sell volume
- Zero line reference (dashed)
- Green when positive, red when negative
- Filled area chart

#### 5. **Zero-Cross Detection & Alerts** (KEY ENTRY SIGNAL)
- **Minimum threshold:** ±300 contracts (prevents noise)
- **Audio alert:** Bullish = 800Hz, Bearish = 400Hz
- **Screen flash:** Green/red border pulse (0.5s)
- **Badge notification:** 3-second center display
- **Vertical markers:** Dashed lines on canvas with labels
- **Console logging:** Full details of each cross

#### 6. **CVD Header Widget**
- Always-visible current CVD value
- Direction indicator (↗ BULLISH / ↘ BEARISH)
- Color-coded border (green/red)
- Age indicator ("Since 9:30 AM")
- Real-time updates

#### 7. **CVD Reset System**
- **Manual reset:** 🔄 button in header (rotates on hover)
- **Session auto-reset:** 9:30 AM, 12:00 PM, 3:00 PM ET
- **Console notifications:** Logs next reset time
- **Timer scheduling:** Automatic daily cycle

#### 8. **Demo Mode**
- Synthetic trade generation
- Large trade bias (10-300 contracts)
- Realistic price movement
- No API required

---

## 🚧 PHASE 2: Level Management & Key Levels

### **Planned Features:**

#### 1. **VWAP Bands** ⭐⭐⭐
- Standard VWAP line
- 1σ bands (standard deviation)
- 2σ bands
- Color-coded (blue/purple)
- **Purpose:** Entry/exit levels, support/resistance

#### 2. **Prior Day Levels**
- Prior Day High (PDH)
- Prior Day Low (PDL)
- Prior Day Close (PDC)
- Dotted horizontal lines
- **Purpose:** Key breakout/breakdown levels

#### 3. **Overnight Levels**
- Overnight High (ONH)
- Overnight Low (ONL)
- Session opening range
- **Purpose:** Initial bias, trap detection

#### 4. **Value Area Lines**
- Value Area High (VAH)
- Value Area Low (VAL)
- Point of Control (POC)
- From prior session's volume profile
- **Purpose:** Fair value zones

#### 5. **Session Time Markers**
- Vertical lines at session boundaries
- Background shading for prime hours
- **Sessions:**
  - London Open (3:00 AM ET)
  - NY Open (9:30 AM ET) - PRIMARY
  - Final Hour (3:00 PM ET) - SECONDARY
- Current time indicator

#### 6. **Manual Level Drawing**
- Click to add horizontal lines
- Custom labels
- Delete/edit capability
- **Purpose:** Support/resistance, entry levels

#### 7. **Level Interaction Alerts**
- Price approaching level (within 5 points)
- Price touching level
- Price breaking level
- Visual + audio notifications

---

## 🔮 PHASE 3: Advanced Analytics

### **Planned Features:**

#### 1. **Volume Profile Sidebar** ⭐⭐⭐
- Horizontal histogram (right side)
- Shows volume at each price level
- **Identify:**
  - LVNs (Low Volume Nodes) - thin areas, price moves fast
  - POC (Point of Control) - highest volume, magnet/target
  - Value Area - 70% of volume
- Apply to custom ranges (impulse legs)
- Real-time updates

#### 2. **Divergence Detection**
- **Bullish divergence:** Price makes lower low, CVD makes higher low
- **Bearish divergence:** Price makes higher high, CVD makes lower high
- Visual markers on price chart
- Alert notifications
- **Purpose:** Reversal warnings

#### 3. **Absorption Detection**
- Heavy volume at price level + price holds
- Identifies institutional positioning
- Yellow highlight zones on price grid
- Alert: "Absorption at [price]"
- **Purpose:** Find where "smart money" is absorbing

#### 4. **Market State Indicator**
- **Balance:** Rotating around fair value (70% of time)
- **Imbalance - Bullish:** Directional push up
- **Imbalance - Bearish:** Directional push down
- Top-left badge showing current state
- **Purpose:** Know which model to use (Trend vs Mean Reversion)

#### 5. **Impulse Leg Profiler**
- Automatic detection of impulse moves
- 5-question validation (Fabio's method)
- Auto-apply volume profile to leg
- Mark LVNs from impulse
- **Purpose:** Find exact entry levels

#### 6. **Delta Ribbon**
- Multi-timeframe delta (1m, 5m, 15m)
- Color-coded ribbon chart
- Shows delta alignment across timeframes
- **Purpose:** Confluence trading

#### 7. **Trade Log Panel**
- Scrolling list of recent trades
- Color-coded by side
- Size and price displayed
- Filter by size
- **Purpose:** Traditional tape reading

#### 8. **Sound Alerts System**
- Configurable alerts for:
  - CVD zero crosses
  - Large trades (100+, 200+, 500+)
  - Stacked imbalances
  - Level touches
  - Divergences
- Volume control
- Enable/disable per alert type

#### 9. **Settings Panel**
- CVD threshold adjustment
- Session reset times
- Alert preferences
- Visual preferences (colors, sizes)
- Export/import settings

---

## 🎯 PHASE 4: Data Integration & Production

### **Planned Features:**

#### 1. **Databento Integration** (Real Market Data)
- Connect to Databento API
- Stream live CME Globex trades (NQ, ES)
- Replace demo mode
- **Cost:** ~$0.02/min (~$2.40/hour for NQ+ES)

#### 2. **Tradovate Execution**
- Connect to Tradovate broker API
- Place orders directly from chart
- One-click trading at levels
- Position management
- **Purpose:** Execution platform integration

#### 3. **Historical Replay**
- Load historical trade data
- Replay at variable speed (1x, 5x, 10x)
- Practice on past sessions
- **Purpose:** Training without live risk

#### 4. **Multi-Instrument Support**
- Switch between NQ, ES, RTY, YM
- Multi-window support
- Synchronized CVD across instruments
- **Purpose:** Trade multiple markets

#### 5. **Session Recording**
- Record entire trading sessions
- Playback with all indicators
- Screenshot/clip capability
- **Purpose:** Journal trades, study patterns

#### 6. **Performance Metrics**
- CVD flip accuracy tracking
- Win rate by setup type
- Best session times
- Dashboard view
- **Purpose:** Optimize strategy

---

## 📋 Feature Prioritization (User Adjustable)

### **Critical (Must Have):**
- ✅ Bubbles visualization
- ✅ CVD with zero-cross alerts
- ✅ Stacked imbalances
- ✅ Large trade tiers
- 🔲 VWAP bands
- 🔲 Volume Profile sidebar
- 🔲 Session time markers

### **High Priority (Should Have):**
- ✅ CVD reset system
- 🔲 Prior day levels
- 🔲 Divergence detection
- 🔲 Absorption detection
- 🔲 Settings panel

### **Medium Priority (Nice to Have):**
- 🔲 Manual level drawing
- 🔲 Market state indicator
- 🔲 Delta ribbon
- 🔲 Trade log panel
- 🔲 Historical replay

### **Low Priority (Future):**
- 🔲 Multi-instrument
- 🔲 Session recording
- 🔲 Performance metrics

---

## 🛠️ Technical Debt & Improvements

### **Performance:**
- Optimize bubble rendering for >1000 bubbles
- Canvas layer separation (static vs animated)
- Web Worker for CVD calculations
- Virtualization for trade log

### **Code Quality:**
- Separate CVD logic into custom hook
- Extract alert system to service
- TypeScript strict mode
- Unit tests for CVD calculations

### **UX Improvements:**
- Keyboard shortcuts
- Touch support for mobile
- Responsive design for tablets
- Dark/light theme toggle

---

## 📖 Fabio Valentini's 3-Element Framework

**Every trade requires ALL 3 elements:**

| Element | Question | Tools in FLOW |
|---------|----------|---------------|
| **1. Market State** | Balance or Imbalance? | Phase 3: Market State Indicator |
| **2. Location** | At key level? | Phase 2: VWAP, PDH/PDL, LVNs |
| **3. Aggression** | Buyers/sellers showing up? | ✅ Phase 1: Bubbles, CVD flip, Stacked imbalances |

---

## 🎓 Trading Models Supported

### **Model 1: Trend Model (Imbalance Continuation)**
**Current Support:**
- ✅ CVD flip detection
- ✅ Stacked imbalances
- ✅ Large trade detection
- 🔲 LVN markers (Phase 2/3)

**Workflow:**
1. Identify impulse leg (Phase 3: Auto-detect)
2. Profile leg for LVNs (Phase 3: Volume Profile)
3. Wait for pullback to LVN
4. Watch for aggression in trend direction (✅ Phase 1)
5. Enter on CVD flip (✅ Phase 1)

### **Model 2: Mean Reversion (Failed Breakout)**
**Current Support:**
- ✅ CVD flip detection
- ✅ Large trade absorption
- 🔲 Balance area POC (Phase 2/3)

**Workflow:**
1. Identify failed breakout
2. Wait for reclaim into balance
3. Profile reclaim leg (Phase 3)
4. Pullback to LVN
5. CVD flip + aggression = entry (✅ Phase 1)

---

## 💾 Version History

### v0.1.0 - Phase 1 Complete (Current)
- Real-time bubbles
- CVD with zero-cross alerts
- Stacked imbalances
- Large trade tiers
- Session-based resets
- Demo mode

### v0.2.0 - Phase 2 (Planned)
- VWAP bands
- Prior day levels
- Session markers
- Manual level drawing

### v0.3.0 - Phase 3 (Planned)
- Volume Profile
- Divergence detection
- Market state indicator
- Settings panel

### v1.0.0 - Production (Planned)
- Databento integration
- Full Fabio strategy support
- Performance optimized
- Complete documentation

---

## 📚 Resources

### **Strategy:**
- TradeZella Playbook: https://tradezella.com/playbooks/auction-market-playbook
- Fabio LinkedIn: https://linkedin.com/in/fabervaale
- World Class Edge: https://worldclassedge.com

### **Data Providers:**
- Databento: https://databento.com
- Tradovate: https://tradovate.com

### **Technologies:**
- React 18 + TypeScript
- Canvas API for rendering
- Vite for bundling
- Rust (optional backend)

---

**Last Updated:** December 19, 2024
**Status:** Phase 1 Complete ✅
**Next:** Phase 2 - Level Management
