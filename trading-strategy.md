# Fabio Valentini Nasdaq Orderflow Strategy â€” Complete Guide (v2)

*Updated with research-verified corrections from TradeZella playbook, Titans of Tomorrow podcast, and Andrea Cimitan interviews.*

## Who Is Fabio Valentini?

Italian professional scalper based in Dubai, consistently ranked in the **top 0.5% of CME Group futures traders**. Verified results from Robbins World Cup Trading Championships:
- 69% return (1st competition)
- 90% return (2nd competition)  
- **218% return** (3rd competition)
- 160% return (4th competition)

Over **2,000 trades in 12 months** with drawdowns under 20%. Co-created **DeepCharts** (rebranded Volumetrica platform).

---

## The Core Principle

> **"I don't try to catch the absolute low or high â€” I join the market at high-probability moments when participants have revealed themselves."**

The strategy exploits **trapped traders** at specific price levels where one side previously showed dominance, confirmed by real-time aggression.

---

## Critical Distinction: Reversal vs. Mean Reversion

âš ï¸ **These are NOT the same thing. Fabio explicitly avoids "reversals" but uses "mean reversion."**

| Concept | Definition | Fabio's Stance |
|---------|------------|----------------|
| **Reversal** | Trying to catch the absolute high or low. Anticipating the turn before confirmation. "Falling knife" trades. | **AVOID** â€” Only attempt after already profitable for the day |
| **Mean Reversion** | Trading AFTER a breakout has already failed and price has reclaimed prior balance. Waiting for confirmation that trapped traders exist. | **USE** â€” Core strategy for balanced/ranging markets |

> **"Avoid reversals until you're green"** â€” Fabio's rule means only take riskier failed-auction setups after securing profit for the day.

---

## The Three-Element Framework

**ALL THREE must align before entry. No exceptions.**

| Element | Question | What You're Looking For |
|---------|----------|------------------------|
| **1. Market State** | Is the market balanced or imbalanced? | Balance = rotating around fair value (70% of time). Imbalance = directional push seeking new fair value |
| **2. Location** | Am I at a level where trapped traders will panic? | LVNs, POC, VWAP bands, prior day high/low, overnight high/low |
| **3. Aggression** | Are buyers/sellers ACTUALLY showing up right now? | Large bubbles, stacked imbalances, delta flip, absorption |

---

## Two Trading Models

### Model 1: Trend Model (Imbalance Continuation)
**When to use:** Market has broken structure and is trending  
**Best session:** New York open (9:30 AM - 12:00 PM ET)  
**Avoid:** London open â€” too many fake breakouts

**Execution:**
1. Confirm market is **out of balance** â€” you should see displacement and momentum away from prior value
2. If price is just rotating up and down, **skip this setup**
3. Identify the impulse leg that broke prior structure
4. Apply Volume Profile to that leg â†’ find internal LVNs
5. Set alerts just before LVN levels (never blind limit orders)
6. When price pulls back to LVN, watch for aggression in trend direction
7. Enter with stop 1-2 ticks beyond the LVN
8. **Target:** Previous balance area POC

### Model 2: Mean Reversion Model (Failed Breakout Snap-back)
**When to use:** Breakout fails and price reclaims prior balance area  
**Best session:** London session (3:00 AM - 8:00 AM ET) or compressed market conditions  
**Risk timing:** Best taken after already profitable for the day

**Execution:**

> âš ï¸ **"Do not take the first move back; that's risky."** â€” Official TradeZella Playbook

1. **Market State:** Confirm market is in balance or consolidation. Use previous day's profile as balance reference.
2. **Watch for failure:** Price pushes out of balance and then **fails** (you're not predicting â€” you're confirming)
3. **Wait for TWO conditions:**
   - a) A **clear reclaim** back inside balance area (price firmly returns inside value)
   - b) A **pullback** into the reclaim leg (do NOT enter on first move back)
4. Apply Volume Profile to the reclaim leg â†’ find internal LVNs
5. On pullback to LVN, check for aggression in snap-back direction
6. Enter once aggression confirms
7. Stop just beyond the failed high/low (add 1-2 tick buffer)
8. **Target:** Balance area POC (center of value â€” don't stretch for other side of range)

---

## Entry Triggers (What Must Appear)

### For Longs at predetermined levels:
- âœ… Large green bubbles/prints at support (Prism shows this)
- âœ… Buy imbalances on footprint at LVN
- âœ… CVD delta flip turning positive through zero
- âœ… Absorption: Heavy volume at level, price holds

### For Shorts (inverse):
- âœ… Large red bubbles/prints at resistance
- âœ… Sell imbalances on footprint at LVN
- âœ… CVD delta flip turning negative through zero
- âœ… Absorption: Heavy selling volume, price holds

### Critical Rule:
> **"No aggression = No trade"**
> **"Enter only when aggression shows up."**
> Wait for buyers/sellers to reveal themselves â€” never anticipate.

---

## Stop Loss Placement

Stops are **structure-based**, not arbitrary tick counts:

1. Place stop **1-2 ticks beyond the aggressive print** that triggered entry
2. Add small buffer before obvious swing highs/lows
3. **If footprint loses pressure within 2-3 bars** â†’ scratch to breakeven immediately

> **"If you're wrong, you should be wrong immediately. Never widen the stop."**

---

## Take Profit Targets

| Model | Target | Notes |
|-------|--------|-------|
| Trend Model | Previous balance POC | Price seeking new fair value |
| Mean Reversion | Balance area POC | Center of value â€” don't stretch for other side |

**Scaling:**
- First scale-out at **+1R** to remove emotional pressure
- Trail remaining position to last absorption or VWAP band after +1.5R

**Typical R-multiples:** 1:2.5 to 1:5

---

## Exit Signals (Abort Trade Regardless of P&L)

- âŒ Aggressive unwind with delta divergence + opposite stacked imbalances
- âŒ Clean VWAP reclaim against your bias
- âŒ Delta flip against your position
- âŒ Price reclaims invalidation level and holds for 2 bars

---

## Position Sizing: A/B/C System

| Grade | Description | Risk Allocation |
|-------|-------------|-----------------|
| **A Setup** | All 3 elements + multiple tape confirmations | Maximum (0.5-1% of account) |
| **B Setup** | Structure + 1 tape signal (imbalance OR delta, not both) | Half of maximum |
| **C Setup** | Structure only, tape pending â€” enter small, quick scratch if no confirm | Quarter of maximum |

**Dynamic risk principle:** Start each session at 0.25% risk per trade. Use early profits to fund larger positions. Worst losses come on small size; largest positions ride already-profitable days.

---

## Session Timing (All Times Eastern)

| Session | Time (ET) | Time (PT) | Trading Approach |
|---------|-----------|-----------|------------------|
| **London Open** | 3:00 - 4:00 AM | 12:00 - 1:00 AM | âš ï¸ High fakeout risk â€” observe only |
| **London Active** | 4:00 - 8:00 AM | 1:00 - 5:00 AM | Mean reversion setups only |
| **NY Open** | 9:30 AM - 12:00 PM | 6:30 - 9:00 AM | **PRIMARY** â€” Trend Model focus |
| **Midday** | 12:00 - 3:00 PM | 9:00 AM - 12:00 PM | Avoid â€” lunch chop |
| **Final Hour** | 3:00 - 4:00 PM | 12:00 - 1:00 PM | Secondary â€” only if structure intact |

### Your Trading Windows (Pacific Time):
- **Primary:** 6:30 AM - 9:00 AM PT (NY open â€” Trend Model)
- **Secondary:** 12:00 PM - 1:00 PM PT (Final hour)
- **Supplementary:** 1:00 AM - 4:00 AM PT (London â€” Mean Reversion only)

---

## London Session Deep Dive

### Why Trade London?

The official playbook says "avoid London open â€” too many fake breakouts" for the **Trend Model**. However, these same fakeouts become the **opportunity** for Mean Reversion.

**London session fake breakouts = trapped traders = fuel for mean reversion trades.**

### London Session Warnings

From the official TradeZella playbook:

- âš ï¸ **"Requires full attention"** â€” not suitable for casual or part-time trading
- âš ï¸ **"Lower win rate in choppy markets"** â€” expect more failed trades on compressed/indecisive days
- âš ï¸ **"Mentally demanding"** â€” multiple small stop-outs in a row are common
- âš ï¸ **Fabio himself doesn't trade London regularly** â€” in the Andrea Cimitan video, he was "not even used to trading the London session"

### London Session Best Practices

1. **Only use Mean Reversion Model** â€” never Trend Model
2. **Wait for fakeouts to complete** â€” don't trade the first 30-60 minutes
3. **Best window: 4:00 - 7:00 AM ET (1:00 - 4:00 AM PT)** â€” after initial chaos settles
4. **Consider it supplementary** â€” your primary edge is still NY session
5. **Ideally trade London after a profitable NY session** â€” aligns with "avoid reversals until you're green"

### London Pre-Session Checklist

- [ ] Mark prior RTH session Value Area (VAH, VAL, POC)
- [ ] Identify balance reference (usually prior day's profile)
- [ ] Note current market state: balanced or imbalanced?
- [ ] Set alerts at balance area edges (VAH/VAL)
- [ ] Have orderflow tool ready (Prism, footprint, or custom tool)
- [ ] **Determine if you're already "green" for the day** â€” affects whether to take Mean Reversion setups

---

## No-Trade Conditions

Stop trading when you observe:
- âŒ Inside-day chop around prior Value Area
- âŒ Overlapping VWAPs (indecision)
- âŒ News-driven whips without absorption footprints
- âŒ Price ping-ponging mid-VWAP with no imbalance
- âŒ 2-5 minutes before/after Tier-1 economic releases
- âŒ First 30-60 minutes of London session (high fakeout risk)

> **"If both sides are dead, simply don't force it."**

---

## Daily Loss Limits

- **Daily loss limit:** 2-3% of account maximum
- **Stop trading at 50-60% of daily limit** â€” elite traders exit early
- **Max consecutive losses:** 3 stop-outs â†’ mandatory 30-minute reset
- **Loss-from-top:** After building profits, don't let day become significant loss

---

## Impulse Leg Identification (What to Profile)

### 5-Question Test:
1. Did it break prior swing high/low? (Yes = impulse)
2. Was it fast (3-5 candles max)? (Yes = impulse)
3. Candles mostly one color, little overlap? (Yes = impulse)
4. Volume increased on move? (Yes = impulse)
5. Move â‰¥30-50 points on NQ? (Yes = impulse)

**Scoring:** 4-5 yes = profile it | 2-3 yes = weak/skip | 0-1 yes = chop

### NQ Rules of Thumb:
| Move Size | Action |
|-----------|--------|
| 100+ points | Definitely profile (major impulse) |
| 50-100 points | Profile if fast and clean |
| 30-50 points | Only if very clean |
| <30 points | Skip â€” LVNs too tight |

> **"Obvious Test":** If you have to explain why it's an impulse, it's too small.

---

# YOUR EXACT TOOL SETUP

## Platform Stack

| Purpose | Tool | Cost |
|---------|------|------|
| **Charts + Analysis** | TradingView Premium | ~$60/month (you have this) |
| **Real-time Orderflow** | Prism / Custom FLOW tool | (your existing/building) |
| **Execution** | Tradovate (via prop firm) | Included |
| **Funding** | Prop Firm (Apex or MFF) | $77-167/month |

## TradingView Chart Layout (3 Charts)

### Chart 1: Left â€” 1-Minute Execution Chart
- Candlestick chart
- VWAP with bands (built-in)
- Prism on second monitor for bubble confirmation

### Chart 2: Top-Right â€” 1-Minute with Fixed Range Volume Profile
- Add "Fixed Range Volume Profile" drawing tool
- Use to profile impulse legs
- Mark LVNs manually with horizontal lines

### Chart 3: Bottom-Right â€” 15-Minute Structure
- Session Volume Profile (prior day)
- Auction Market Levels indicator (custom Pine Script)
- Shows: PDH, PDL, ONH, ONL

---

## TradingView Indicators to Add

### Built-in (Free):
1. **VWAP** â€” enable standard deviation bands (1Ïƒ, 2Ïƒ)
2. **Volume Profile Visible Range** â€” Row size: 100, Value Area: 70%
3. **Fixed Range Volume Profile** â€” drawing tool for impulse legs

### Custom Pine Scripts (Created for You):
1. **Auction Market Levels** â€” auto-plots PDH/PDL/ONH/ONL
2. **CVD with Divergence Detection** â€” shows delta divergence
3. **Market State Detector** â€” identifies balance vs imbalance

### Community (Search in Indicators):
- "Cumulative Volume Delta" by LuxAlgo (or similar)
- "Delta Volume" for candle-by-candle delta

---

## Pre-Session Checklist (Before 9:30 AM ET / 6:30 AM PT)

- [ ] Mark prior day High/Low/Close
- [ ] Mark overnight High/Low
- [ ] Identify balance vs imbalance (did price break structure overnight?)
- [ ] Apply Volume Profile to any overnight impulse legs â†’ mark LVNs
- [ ] Set alerts 2-3 ticks before key levels
- [ ] Determine bias: bullish, bearish, or neutral (wait for clarity if neutral)

---

## Execution Workflow

### Trend Model (NY Session):
```
1. TradingView alerts you: Price approaching your level
         â†“
2. Quick glance at Prism: Large bubbles appearing?
         â†“
   YES â†’ Continue          NO â†’ Wait/Skip
         â†“
3. Check CVD: Delta flipping in trade direction?
         â†“
   YES â†’ Execute           NO â†’ Wait for confirmation
         â†“
4. Place order in TradingView (connected to Tradovate)
         â†“
5. Manage trade: Stop 1-2 ticks beyond entry trigger
         â†“
6. Target: POC of prior balance area
```

### Mean Reversion Model (London Session):
```
1. Identify balance area (prior day's profile)
         â†“
2. Watch for breakout attempt â†’ Does it FAIL?
         â†“
   NO â†’ Skip (use Trend Model if trending)
         â†“
   YES â†’ Continue
         â†“
3. Wait for CLEAR RECLAIM inside balance
         â†“
4. Wait for PULLBACK into reclaim leg (NOT first move back!)
         â†“
5. Apply Volume Profile to reclaim leg â†’ Mark LVNs
         â†“
6. At LVN: Check orderflow for aggression in snap-back direction
         â†“
   Aggression confirmed â†’ Enter
         â†“
7. Stop: Just beyond failed high/low
         â†“
8. Target: Balance POC (center of value)
```

---

# ACCOUNT SETUP STEPS

## Step 1: Free Practice (Before Paying)

1. Go to **tradovate.com** â†’ Create free demo account
2. Connect Tradovate to TradingView:
   - TradingView â†’ Trading Panel (bottom) â†’ Connect Broker â†’ Tradovate
3. Practice for 2-4 weeks with paper trading
4. Focus on identifying setups, not profits

## Step 2: Prop Firm Evaluation (When Ready)

**Recommended: Apex Trader Funding or My Funded Futures**

| Step | Action |
|------|--------|
| 1 | Sign up at apextraderfunding.com or myfundedfutures.com |
| 2 | Purchase evaluation (~$77-167, watch for 80% off promos) |
| 3 | Receive Tradovate credentials via email |
| 4 | Connect to TradingView (same process as demo) |
| 5 | Begin evaluation when ready (no time limit) |

---

## Contract Specifications

| Contract | Tick Value | Point Value | 10-Point Stop Cost |
|----------|-----------|-------------|-------------------|
| **NQ** (E-mini Nasdaq) | $5.00 | $20.00 | $200 |
| **MNQ** (Micro Nasdaq) | $0.50 | $2.00 | $20 |

**Ratio:** 1 NQ = 10 MNQ in exposure

**Start with MNQ** until consistently profitable.

---

# 30-DAY ONBOARDING PLAN

## Week 1: Setup & Study (No Trading)
- Day 1-2: Download TradeZella playbook (tradezella.com/playbooks/auction-market-playbook)
- Day 3-4: Set up TradingView with all indicators
- Day 5-7: Watch market during NY session, practice marking levels
- **Goal:** Identify balance vs imbalance correctly

## Week 2: Paper Trading
- Sign up for free Tradovate demo
- Trade 1 MNQ contract only
- Focus on identifying setups, not winning
- Mark every LVN on impulse legs
- **Goal:** Execute 20+ practice trades

## Week 3: Pattern Recognition
- Document every setup (screenshot + notes)
- Track: Which patterns worked? Which didn't?
- Which aggression signal works best for you?
- **Goal:** Identify your strongest setup type

## Week 4: System Refinement
- Review all trades from Week 3
- Consider starting prop firm evaluation
- Trade only A-setups initially
- **Goal:** Break-even or small profit

---

# KEY RESOURCES

## Free
- **TradeZella Playbook:** tradezella.com/playbooks/auction-market-playbook
- **YouTube:** "I Traded with the World #1 Scalper" (Andrea Cimitan channel)
- **LinkedIn:** linkedin.com/in/fabervaale (posts NQ model performance)
- **Podcast:** Titans of Tomorrow â€” "World's #1 Scalper" episode

## Paid
- **World Class Edge:** worldclassedge.com (live trading floor, ~$200-500/month)
- **Morpheus Education:** morpheus.education (Italian-focused, has English content)

---

# QUICK REFERENCE CARD

## Entry Checklist:
âœ… Market State identified (Balance/Imbalance)  
âœ… At predetermined level (LVN, POC, PDH/PDL, ONH/ONL, VWAP band)  
âœ… Aggression confirmed (bubbles, delta flip, stacked imbalances)  
âœ… Stop placement defined BEFORE entry  
âœ… Target identified (POC)

## Model Selection:
| Condition | Model | Session |
|-----------|-------|---------|
| Market trending, out of balance | Trend Model | NY Open |
| Failed breakout, back in balance | Mean Reversion | London or NY |

## Mean Reversion Specific:
âš ï¸ Do NOT take first move back â€” wait for reclaim + pullback  
âš ï¸ Best after already profitable ("avoid reversals until green")  
âš ï¸ Target balance POC â€” don't stretch for other side

## Exit Rules:
- Stop: 1-2 ticks beyond entry trigger
- Target: Previous/current balance POC
- Scale: First partial at +1R
- Abort: Delta flip against, VWAP reclaim against, invalidation held 2 bars

## Position Size:
- A Setup: Full size (0.5-1% risk)
- B Setup: Half size
- C Setup: Quarter size (scratch if no confirm)

## Session Focus:
- Primary: 6:30 AM - 9:00 AM PT (NY open)
- Secondary: 12:00 PM - 1:00 PM PT (final hour)
- Supplementary: 1:00 AM - 4:00 AM PT (London â€” Mean Reversion only)
- Avoid: Pre-market, mid-day chop, news releases, first 30 min of London

---

# CHANGELOG (v2)

## Corrections from Research Verification:

1. **Added "Reversal vs Mean Reversion" distinction** â€” Fabio explicitly avoids reversals but uses mean reversion. These are different concepts.

2. **Fixed Mean Reversion execution steps** â€” Added the critical warning: "Do not take the first move back; that's risky." Must wait for (1) clear reclaim AND (2) pullback into reclaim leg.

3. **Added risk timing rule** â€” "Avoid reversals until you're green" â€” Mean Reversion setups are best taken after securing profit for the day.

4. **Expanded London Session section** â€” Added detailed timing, warnings, and best practices based on official playbook language.

5. **Updated session timing table** â€” Added London session breakdown with PT times for your schedule.

6. **Added separate Mean Reversion workflow** â€” Distinct from Trend Model workflow to avoid confusion.

7. **Updated No-Trade conditions** â€” Added "First 30-60 minutes of London session" as high fakeout risk period.

---

*Strategy based on Fabio Valentini's publicly available methodology from TradeZella playbook, Titans of Tomorrow podcast interviews, Andrea Cimitan YouTube content, and World Class Edge materials.*

*Last updated: December 2024*
