-- ================================================
-- HITTHEBID JOURNAL SCHEMA
-- Supabase PostgreSQL schema for trade journaling
-- ================================================

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ================================================
-- TRADING SESSIONS TABLE
-- One row per trading day
-- ================================================
CREATE TABLE IF NOT EXISTS trading_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,

    -- Date (unique per user per day)
    date DATE NOT NULL,

    -- Pre-market levels
    pdh DECIMAL(10,2),           -- Prior Day High
    pdl DECIMAL(10,2),           -- Prior Day Low
    pdc DECIMAL(10,2),           -- Prior Day Close
    onh DECIMAL(10,2),           -- Overnight High
    onl DECIMAL(10,2),           -- Overnight Low
    poc DECIMAL(10,2),           -- Point of Control
    vah DECIMAL(10,2),           -- Value Area High
    val DECIMAL(10,2),           -- Value Area Low
    lvn_levels DECIMAL(10,2)[],  -- Array of LVN levels

    -- Daily setup
    premarket_bias TEXT CHECK (premarket_bias IN ('bullish', 'bearish', 'neutral')) DEFAULT 'neutral',
    market_state_at_open TEXT CHECK (market_state_at_open IN ('balance', 'imbalance')),
    daily_thesis TEXT,

    -- Post-session review
    daily_review TEXT,
    tomorrow_focus TEXT,
    lessons_learned TEXT,

    -- Computed stats (updated by triggers)
    total_trades INTEGER DEFAULT 0,
    winners INTEGER DEFAULT 0,
    losers INTEGER DEFAULT 0,
    scratches INTEGER DEFAULT 0,
    gross_profit DECIMAL(10,2) DEFAULT 0,
    gross_loss DECIMAL(10,2) DEFAULT 0,
    net_pnl DECIMAL(10,2) DEFAULT 0,

    -- Prop firm tracking
    equity_high DECIMAL(10,2) DEFAULT 0,
    max_drawdown_from_high DECIMAL(10,2) DEFAULT 0,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Ensure one session per user per day
    UNIQUE(user_id, date)
);

-- Index for fast date lookups
CREATE INDEX IF NOT EXISTS idx_sessions_user_date ON trading_sessions(user_id, date DESC);

-- ================================================
-- TRADES TABLE
-- Individual trade records
-- ================================================
CREATE TABLE IF NOT EXISTS trades (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id UUID REFERENCES trading_sessions(id) ON DELETE CASCADE,
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,

    -- Trade number for the day
    trade_number INTEGER NOT NULL,

    -- Timing
    entry_time TIMESTAMPTZ NOT NULL,
    exit_time TIMESTAMPTZ,

    -- Three Elements (pre-trade analysis)
    market_state TEXT NOT NULL CHECK (market_state IN ('balance', 'imbalance')),
    location_type TEXT NOT NULL CHECK (location_type IN (
        'lvn', 'poc', 'vah', 'val', 'vwap', 'vwap_upper', 'vwap_lower',
        'pdh', 'pdl', 'onh', 'onl', 'other'
    )),
    location_price DECIMAL(10,2) NOT NULL,
    aggression_type TEXT NOT NULL CHECK (aggression_type IN (
        'absorption', 'delta_flip', 'stacked_imbalance', 'big_prints', 'none'
    )),
    prism_confirmation BOOLEAN DEFAULT true,
    setup_grade CHAR(1) NOT NULL CHECK (setup_grade IN ('A', 'B', 'C')),

    -- Execution plan
    direction TEXT NOT NULL CHECK (direction IN ('long', 'short')),
    entry_price DECIMAL(10,2) NOT NULL,
    stop_price DECIMAL(10,2) NOT NULL,
    target_price DECIMAL(10,2) NOT NULL,
    position_size INTEGER NOT NULL DEFAULT 1,
    planned_rr DECIMAL(5,2),

    -- Result (filled after trade closes)
    exit_price DECIMAL(10,2),
    exit_type TEXT CHECK (exit_type IN ('target', 'stop', 'scratch', 'manual', 'trailing')),
    pnl DECIMAL(10,2),
    pnl_points DECIMAL(10,2),
    actual_rr DECIMAL(5,2),

    -- Trade state
    is_open BOOLEAN DEFAULT true,

    -- Notes
    entry_notes TEXT,
    exit_notes TEXT,
    what_worked TEXT,
    what_to_improve TEXT,
    screenshot TEXT,  -- URL to screenshot

    -- Signal source (for auto-captured trades)
    signal_source TEXT CHECK (signal_source IN ('manual', 'paper_trade', 'backtest')),
    signal_id TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for fast lookups
CREATE INDEX IF NOT EXISTS idx_trades_session ON trades(session_id);
CREATE INDEX IF NOT EXISTS idx_trades_user ON trades(user_id);
CREATE INDEX IF NOT EXISTS idx_trades_entry_time ON trades(entry_time DESC);
CREATE INDEX IF NOT EXISTS idx_trades_is_open ON trades(is_open) WHERE is_open = true;

-- ================================================
-- FUNCTIONS & TRIGGERS
-- Auto-update session stats when trades change
-- ================================================

-- Function to update session stats
CREATE OR REPLACE FUNCTION update_session_stats()
RETURNS TRIGGER AS $$
DECLARE
    v_session_id UUID;
    v_total INTEGER;
    v_winners INTEGER;
    v_losers INTEGER;
    v_scratches INTEGER;
    v_gross_profit DECIMAL(10,2);
    v_gross_loss DECIMAL(10,2);
    v_equity_high DECIMAL(10,2);
BEGIN
    -- Get session_id from the affected row
    IF TG_OP = 'DELETE' THEN
        v_session_id := OLD.session_id;
    ELSE
        v_session_id := NEW.session_id;
    END IF;

    -- Calculate stats
    SELECT
        COUNT(*),
        COUNT(*) FILTER (WHERE pnl > 0),
        COUNT(*) FILTER (WHERE pnl < 0),
        COUNT(*) FILTER (WHERE exit_type = 'scratch'),
        COALESCE(SUM(pnl) FILTER (WHERE pnl > 0), 0),
        COALESCE(ABS(SUM(pnl) FILTER (WHERE pnl < 0)), 0)
    INTO v_total, v_winners, v_losers, v_scratches, v_gross_profit, v_gross_loss
    FROM trades
    WHERE session_id = v_session_id AND is_open = false;

    -- Calculate running equity high
    SELECT GREATEST(COALESCE(MAX(running_equity), 0), 0)
    INTO v_equity_high
    FROM (
        SELECT SUM(pnl) OVER (ORDER BY exit_time) as running_equity
        FROM trades
        WHERE session_id = v_session_id AND is_open = false
    ) subq;

    -- Update session
    UPDATE trading_sessions
    SET
        total_trades = v_total,
        winners = v_winners,
        losers = v_losers,
        scratches = v_scratches,
        gross_profit = v_gross_profit,
        gross_loss = v_gross_loss,
        net_pnl = v_gross_profit - v_gross_loss,
        equity_high = v_equity_high,
        max_drawdown_from_high = v_equity_high - (v_gross_profit - v_gross_loss),
        updated_at = NOW()
    WHERE id = v_session_id;

    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Trigger for trade changes
DROP TRIGGER IF EXISTS trigger_update_session_stats ON trades;
CREATE TRIGGER trigger_update_session_stats
AFTER INSERT OR UPDATE OR DELETE ON trades
FOR EACH ROW EXECUTE FUNCTION update_session_stats();

-- Auto-update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trigger_sessions_updated_at ON trading_sessions;
CREATE TRIGGER trigger_sessions_updated_at
BEFORE UPDATE ON trading_sessions
FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS trigger_trades_updated_at ON trades;
CREATE TRIGGER trigger_trades_updated_at
BEFORE UPDATE ON trades
FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ================================================
-- ROW LEVEL SECURITY
-- Users can only access their own data
-- ================================================

ALTER TABLE trading_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE trades ENABLE ROW LEVEL SECURITY;

-- Sessions policies
CREATE POLICY "Users can view own sessions" ON trading_sessions
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own sessions" ON trading_sessions
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own sessions" ON trading_sessions
    FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own sessions" ON trading_sessions
    FOR DELETE USING (auth.uid() = user_id);

-- Trades policies
CREATE POLICY "Users can view own trades" ON trades
    FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "Users can insert own trades" ON trades
    FOR INSERT WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can update own trades" ON trades
    FOR UPDATE USING (auth.uid() = user_id);

CREATE POLICY "Users can delete own trades" ON trades
    FOR DELETE USING (auth.uid() = user_id);

-- ================================================
-- VIEWS FOR ANALYTICS
-- Pre-computed stats for dashboard
-- ================================================

-- View: User all-time stats
CREATE OR REPLACE VIEW user_stats AS
SELECT
    user_id,
    COUNT(*) as total_trades,
    COUNT(*) FILTER (WHERE pnl > 0) as winners,
    COUNT(*) FILTER (WHERE pnl < 0) as losers,
    ROUND(100.0 * COUNT(*) FILTER (WHERE pnl > 0) / NULLIF(COUNT(*), 0), 1) as win_rate,
    COALESCE(SUM(pnl) FILTER (WHERE pnl > 0), 0) as gross_profit,
    COALESCE(ABS(SUM(pnl) FILTER (WHERE pnl < 0)), 0) as gross_loss,
    COALESCE(SUM(pnl), 0) as net_pnl,
    CASE
        WHEN COALESCE(ABS(SUM(pnl) FILTER (WHERE pnl < 0)), 0) = 0 THEN NULL
        ELSE ROUND(COALESCE(SUM(pnl) FILTER (WHERE pnl > 0), 0) / NULLIF(ABS(SUM(pnl) FILTER (WHERE pnl < 0)), 0), 2)
    END as profit_factor,
    ROUND(AVG(pnl) FILTER (WHERE pnl > 0), 2) as avg_winner,
    ROUND(AVG(ABS(pnl)) FILTER (WHERE pnl < 0), 2) as avg_loser,
    ROUND(AVG(actual_rr), 2) as avg_rr
FROM trades
WHERE is_open = false
GROUP BY user_id;

-- View: Stats by location type
CREATE OR REPLACE VIEW stats_by_location AS
SELECT
    user_id,
    location_type,
    COUNT(*) as count,
    COUNT(*) FILTER (WHERE pnl > 0) as wins,
    COUNT(*) FILTER (WHERE pnl < 0) as losses,
    ROUND(100.0 * COUNT(*) FILTER (WHERE pnl > 0) / NULLIF(COUNT(*), 0), 1) as win_rate,
    COALESCE(SUM(pnl), 0) as net_pnl,
    ROUND(AVG(pnl), 2) as avg_pnl
FROM trades
WHERE is_open = false
GROUP BY user_id, location_type;

-- View: Stats by aggression type
CREATE OR REPLACE VIEW stats_by_aggression AS
SELECT
    user_id,
    aggression_type,
    COUNT(*) as count,
    COUNT(*) FILTER (WHERE pnl > 0) as wins,
    COUNT(*) FILTER (WHERE pnl < 0) as losses,
    ROUND(100.0 * COUNT(*) FILTER (WHERE pnl > 0) / NULLIF(COUNT(*), 0), 1) as win_rate,
    COALESCE(SUM(pnl), 0) as net_pnl,
    ROUND(AVG(pnl), 2) as avg_pnl
FROM trades
WHERE is_open = false
GROUP BY user_id, aggression_type;

-- View: Stats by setup grade
CREATE OR REPLACE VIEW stats_by_grade AS
SELECT
    user_id,
    setup_grade,
    COUNT(*) as count,
    COUNT(*) FILTER (WHERE pnl > 0) as wins,
    COUNT(*) FILTER (WHERE pnl < 0) as losses,
    ROUND(100.0 * COUNT(*) FILTER (WHERE pnl > 0) / NULLIF(COUNT(*), 0), 1) as win_rate,
    COALESCE(SUM(pnl), 0) as net_pnl,
    ROUND(AVG(pnl), 2) as avg_pnl
FROM trades
WHERE is_open = false
GROUP BY user_id, setup_grade;

-- View: Stats by market state
CREATE OR REPLACE VIEW stats_by_market_state AS
SELECT
    user_id,
    market_state,
    COUNT(*) as count,
    COUNT(*) FILTER (WHERE pnl > 0) as wins,
    COUNT(*) FILTER (WHERE pnl < 0) as losses,
    ROUND(100.0 * COUNT(*) FILTER (WHERE pnl > 0) / NULLIF(COUNT(*), 0), 1) as win_rate,
    COALESCE(SUM(pnl), 0) as net_pnl,
    ROUND(AVG(pnl), 2) as avg_pnl
FROM trades
WHERE is_open = false
GROUP BY user_id, market_state;

-- View: Equity curve data
CREATE OR REPLACE VIEW equity_curve AS
SELECT
    user_id,
    date,
    net_pnl as daily_pnl,
    SUM(net_pnl) OVER (PARTITION BY user_id ORDER BY date) as cumulative_equity
FROM trading_sessions
ORDER BY user_id, date;

-- ================================================
-- SAMPLE DATA (optional - for testing)
-- ================================================
-- Run this only if you want to populate test data
-- DELETE FROM trades; DELETE FROM trading_sessions;
--
-- INSERT INTO trading_sessions (user_id, date, pdh, pdl, premarket_bias, daily_thesis)
-- VALUES
--     ('your-user-uuid', '2024-12-20', 21500.00, 21400.00, 'bullish', 'Looking for LVN retests above 21450');
