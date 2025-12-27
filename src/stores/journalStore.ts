import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import {
  Trade,
  TradingSession,
  NewSessionForm,
  NewTradeForm,
  CloseTradeForm,
  SessionStats,
  BreakdownStats,
  ReadyToFundChecklist,
  LocationType,
  AggressionType,
  MarketState,
  SetupGrade,
} from '../journal/types';

// Generate unique ID
const generateId = () => `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

// Get today's date in YYYY-MM-DD format
const getTodayDate = () => new Date().toISOString().split('T')[0];

interface JournalState {
  // Data
  sessions: TradingSession[];
  trades: Trade[];

  // Current session
  currentSessionId: string | null;

  // UI State
  isLoading: boolean;
  error: string | null;

  // Session actions
  createSession: (form: NewSessionForm) => TradingSession;
  updateSession: (id: string, updates: Partial<TradingSession>) => void;
  deleteSession: (id: string) => void;
  setCurrentSession: (id: string | null) => void;
  getSession: (id: string) => TradingSession | undefined;
  getSessionByDate: (date: string) => TradingSession | undefined;
  getTodaySession: () => TradingSession | undefined;

  // Trade actions
  createTrade: (sessionId: string, form: NewTradeForm) => Trade;
  closeTrade: (tradeId: string, form: CloseTradeForm) => void;
  updateTrade: (id: string, updates: Partial<Trade>) => void;
  deleteTrade: (id: string) => void;
  getTradesForSession: (sessionId: string) => Trade[];
  getOpenTrades: () => Trade[];

  // Analytics
  getSessionStats: (sessionId: string) => SessionStats;
  getAllTimeStats: () => SessionStats;
  getStatsByLocation: () => Record<LocationType, BreakdownStats>;
  getStatsByAggression: () => Record<AggressionType, BreakdownStats>;
  getStatsByMarketState: () => Record<MarketState, BreakdownStats>;
  getStatsByGrade: () => Record<SetupGrade, BreakdownStats>;
  getReadyToFundChecklist: () => ReadyToFundChecklist;
  getEquityCurve: () => { date: string; equity: number; drawdown: number }[];

  // Import/Export
  exportData: () => string;
  importData: (json: string) => boolean;
  clearAllData: () => void;
}

// Helper to calculate stats from trades
const calculateStats = (trades: Trade[]): SessionStats => {
  const closedTrades = trades.filter((t) => !t.isOpen && t.pnl !== null);

  if (closedTrades.length === 0) {
    return {
      totalTrades: 0,
      winRate: 0,
      profitFactor: 0,
      avgWinner: 0,
      avgLoser: 0,
      avgRR: 0,
      netPnl: 0,
      maxDrawdown: 0,
      sharpeRatio: 0,
    };
  }

  const winners = closedTrades.filter((t) => (t.pnl ?? 0) > 0);
  const losers = closedTrades.filter((t) => (t.pnl ?? 0) < 0);

  const grossProfit = winners.reduce((sum, t) => sum + (t.pnl ?? 0), 0);
  const grossLoss = Math.abs(losers.reduce((sum, t) => sum + (t.pnl ?? 0), 0));

  const avgWinner = winners.length > 0 ? grossProfit / winners.length : 0;
  const avgLoser = losers.length > 0 ? grossLoss / losers.length : 0;

  const avgRR =
    closedTrades.reduce((sum, t) => sum + (t.actualRR ?? 0), 0) / closedTrades.length;

  // Calculate equity curve for max drawdown
  let equity = 0;
  let peak = 0;
  let maxDrawdown = 0;

  for (const trade of closedTrades) {
    equity += trade.pnl ?? 0;
    if (equity > peak) peak = equity;
    const dd = peak - equity;
    if (dd > maxDrawdown) maxDrawdown = dd;
  }

  // Sharpe ratio (simplified - daily returns)
  const returns = closedTrades.map((t) => t.pnl ?? 0);
  const avgReturn = returns.reduce((a, b) => a + b, 0) / returns.length;
  const stdDev = Math.sqrt(
    returns.reduce((sum, r) => sum + Math.pow(r - avgReturn, 2), 0) / returns.length
  );
  const sharpeRatio = stdDev > 0 ? (avgReturn / stdDev) * Math.sqrt(252) : 0;

  return {
    totalTrades: closedTrades.length,
    winRate: (winners.length / closedTrades.length) * 100,
    profitFactor: grossLoss > 0 ? grossProfit / grossLoss : grossProfit > 0 ? Infinity : 0,
    avgWinner,
    avgLoser,
    avgRR,
    netPnl: grossProfit - grossLoss,
    maxDrawdown,
    sharpeRatio,
  };
};

// Helper to calculate breakdown stats
const calculateBreakdown = (trades: Trade[]): BreakdownStats => {
  const closedTrades = trades.filter((t) => !t.isOpen && t.pnl !== null);
  const winners = closedTrades.filter((t) => (t.pnl ?? 0) > 0);
  const losers = closedTrades.filter((t) => (t.pnl ?? 0) < 0);
  const netPnl = closedTrades.reduce((sum, t) => sum + (t.pnl ?? 0), 0);

  return {
    count: closedTrades.length,
    wins: winners.length,
    losses: losers.length,
    winRate: closedTrades.length > 0 ? (winners.length / closedTrades.length) * 100 : 0,
    netPnl,
    avgPnl: closedTrades.length > 0 ? netPnl / closedTrades.length : 0,
  };
};

export const useJournalStore = create<JournalState>()(
  persist(
    (set, get) => ({
      // Initial state
      sessions: [],
      trades: [],
      currentSessionId: null,
      isLoading: false,
      error: null,

      // Session actions
      createSession: (form) => {
        const now = new Date().toISOString();
        const session: TradingSession = {
          id: generateId(),
          date: form.date,
          pdh: form.pdh ? parseFloat(form.pdh) : null,
          pdl: form.pdl ? parseFloat(form.pdl) : null,
          pdc: form.pdc ? parseFloat(form.pdc) : null,
          onh: form.onh ? parseFloat(form.onh) : null,
          onl: form.onl ? parseFloat(form.onl) : null,
          poc: form.poc ? parseFloat(form.poc) : null,
          vah: form.vah ? parseFloat(form.vah) : null,
          val: form.val ? parseFloat(form.val) : null,
          lvnLevels: form.lvnLevels
            .split(',')
            .map((s) => parseFloat(s.trim()))
            .filter((n) => !isNaN(n)),
          premarketBias: form.premarketBias,
          marketStateAtOpen: null,
          dailyThesis: form.dailyThesis,
          dailyReview: '',
          tomorrowFocus: '',
          lessonsLearned: '',
          totalTrades: 0,
          winners: 0,
          losers: 0,
          scratches: 0,
          grossProfit: 0,
          grossLoss: 0,
          netPnl: 0,
          equityHigh: 0,
          maxDrawdownFromHigh: 0,
          createdAt: now,
          updatedAt: now,
        };

        set((state) => ({
          sessions: [...state.sessions, session],
          currentSessionId: session.id,
        }));

        return session;
      },

      updateSession: (id, updates) => {
        set((state) => ({
          sessions: state.sessions.map((s) =>
            s.id === id ? { ...s, ...updates, updatedAt: new Date().toISOString() } : s
          ),
        }));
      },

      deleteSession: (id) => {
        set((state) => ({
          sessions: state.sessions.filter((s) => s.id !== id),
          trades: state.trades.filter((t) => t.sessionId !== id),
          currentSessionId: state.currentSessionId === id ? null : state.currentSessionId,
        }));
      },

      setCurrentSession: (id) => set({ currentSessionId: id }),

      getSession: (id) => get().sessions.find((s) => s.id === id),

      getSessionByDate: (date) => get().sessions.find((s) => s.date === date),

      getTodaySession: () => get().sessions.find((s) => s.date === getTodayDate()),

      // Trade actions
      createTrade: (sessionId, form) => {
        const sessionTrades = get().trades.filter((t) => t.sessionId === sessionId);
        const now = new Date().toISOString();

        const entryPrice = parseFloat(form.entryPrice);
        const stopPrice = parseFloat(form.stopPrice);
        const targetPrice = parseFloat(form.targetPrice);
        const positionSize = parseInt(form.positionSize) || 1;

        // Calculate planned R:R
        const risk = Math.abs(entryPrice - stopPrice);
        const reward = Math.abs(targetPrice - entryPrice);
        const plannedRR = risk > 0 ? reward / risk : 0;

        const trade: Trade = {
          id: generateId(),
          sessionId,
          tradeNumber: sessionTrades.length + 1,
          entryTime: now,
          exitTime: null,
          marketState: form.marketState,
          locationType: form.locationType,
          locationPrice: parseFloat(form.locationPrice),
          aggressionType: form.aggressionType,
          prismConfirmation: form.prismConfirmation,
          setupGrade: form.setupGrade,
          direction: form.direction,
          entryPrice,
          stopPrice,
          targetPrice,
          positionSize,
          plannedRR,
          exitPrice: null,
          exitType: null,
          pnl: null,
          pnlPoints: null,
          actualRR: null,
          isOpen: true,
          entryNotes: form.entryNotes,
          exitNotes: '',
          whatWorked: '',
          whatToImprove: '',
          screenshot: null,
          signalSource: 'manual',
          signalId: null,
          createdAt: now,
          updatedAt: now,
        };

        set((state) => ({
          trades: [...state.trades, trade],
        }));

        // Update session stats
        get().updateSession(sessionId, {
          totalTrades: sessionTrades.length + 1,
        });

        return trade;
      },

      closeTrade: (tradeId, form) => {
        const trade = get().trades.find((t) => t.id === tradeId);
        if (!trade) return;

        const exitPrice = parseFloat(form.exitPrice);
        const pointsPerDollar = 20; // NQ = $20 per point

        // Calculate P&L
        const priceDiff = exitPrice - trade.entryPrice;
        const pnlPoints =
          trade.direction === 'long' ? priceDiff : -priceDiff;
        const pnl = pnlPoints * pointsPerDollar * trade.positionSize;

        // Calculate actual R:R
        const risk = Math.abs(trade.entryPrice - trade.stopPrice);
        const actualRR = risk > 0 ? pnlPoints / risk : 0;

        const now = new Date().toISOString();

        set((state) => ({
          trades: state.trades.map((t) =>
            t.id === tradeId
              ? {
                  ...t,
                  exitPrice,
                  exitTime: now,
                  exitType: form.exitType,
                  pnl,
                  pnlPoints,
                  actualRR,
                  isOpen: false,
                  exitNotes: form.exitNotes,
                  whatWorked: form.whatWorked,
                  whatToImprove: form.whatToImprove,
                  updatedAt: now,
                }
              : t
          ),
        }));

        // Update session stats
        const sessionTrades = get().trades.filter((t) => t.sessionId === trade.sessionId);
        const closedTrades = sessionTrades.filter((t) => !t.isOpen);
        const winners = closedTrades.filter((t) => (t.pnl ?? 0) > 0);
        const losers = closedTrades.filter((t) => (t.pnl ?? 0) < 0);
        const scratches = closedTrades.filter((t) => t.exitType === 'scratch');
        const grossProfit = winners.reduce((sum, t) => sum + (t.pnl ?? 0), 0);
        const grossLoss = Math.abs(losers.reduce((sum, t) => sum + (t.pnl ?? 0), 0));
        const netPnl = grossProfit - grossLoss;

        // Track equity high and drawdown
        const session = get().getSession(trade.sessionId);
        const equityHigh = Math.max(session?.equityHigh ?? 0, netPnl);
        const maxDrawdownFromHigh = Math.max(
          session?.maxDrawdownFromHigh ?? 0,
          equityHigh - netPnl
        );

        get().updateSession(trade.sessionId, {
          winners: winners.length,
          losers: losers.length,
          scratches: scratches.length,
          grossProfit,
          grossLoss,
          netPnl,
          equityHigh,
          maxDrawdownFromHigh,
        });
      },

      updateTrade: (id, updates) => {
        set((state) => ({
          trades: state.trades.map((t) =>
            t.id === id ? { ...t, ...updates, updatedAt: new Date().toISOString() } : t
          ),
        }));
      },

      deleteTrade: (id) => {
        const trade = get().trades.find((t) => t.id === id);
        set((state) => ({
          trades: state.trades.filter((t) => t.id !== id),
        }));

        // Recalculate session stats if needed
        if (trade) {
          const sessionTrades = get().trades.filter((t) => t.sessionId === trade.sessionId);
          get().updateSession(trade.sessionId, {
            totalTrades: sessionTrades.length,
          });
        }
      },

      getTradesForSession: (sessionId) =>
        get().trades.filter((t) => t.sessionId === sessionId),

      getOpenTrades: () => get().trades.filter((t) => t.isOpen),

      // Analytics
      getSessionStats: (sessionId) => {
        const trades = get().trades.filter((t) => t.sessionId === sessionId);
        return calculateStats(trades);
      },

      getAllTimeStats: () => calculateStats(get().trades),

      getStatsByLocation: () => {
        const trades = get().trades;
        const locations: LocationType[] = [
          'lvn', 'poc', 'vah', 'val', 'vwap', 'vwap_upper', 'vwap_lower',
          'pdh', 'pdl', 'onh', 'onl', 'other',
        ];

        return locations.reduce((acc, loc) => {
          acc[loc] = calculateBreakdown(trades.filter((t) => t.locationType === loc));
          return acc;
        }, {} as Record<LocationType, BreakdownStats>);
      },

      getStatsByAggression: () => {
        const trades = get().trades;
        const types: AggressionType[] = [
          'absorption', 'delta_flip', 'stacked_imbalance', 'big_prints', 'none',
        ];

        return types.reduce((acc, type) => {
          acc[type] = calculateBreakdown(trades.filter((t) => t.aggressionType === type));
          return acc;
        }, {} as Record<AggressionType, BreakdownStats>);
      },

      getStatsByMarketState: () => {
        const trades = get().trades;
        const states: MarketState[] = ['balance', 'imbalance'];

        return states.reduce((acc, state) => {
          acc[state] = calculateBreakdown(trades.filter((t) => t.marketState === state));
          return acc;
        }, {} as Record<MarketState, BreakdownStats>);
      },

      getStatsByGrade: () => {
        const trades = get().trades;
        const grades: SetupGrade[] = ['A', 'B', 'C'];

        return grades.reduce((acc, grade) => {
          acc[grade] = calculateBreakdown(trades.filter((t) => t.setupGrade === grade));
          return acc;
        }, {} as Record<SetupGrade, BreakdownStats>);
      },

      getReadyToFundChecklist: () => {
        const stats = get().getAllTimeStats();
        const trades = get().trades.filter((t) => !t.isOpen && t.pnl !== null);

        // Find largest single loss
        const largestLoss = trades.reduce(
          (max, t) => Math.min(max, t.pnl ?? 0),
          0
        );

        // Find worst daily P&L
        const dailyPnls = get().sessions.map((s) => s.netPnl);
        const worstDay = dailyPnls.reduce((min, pnl) => Math.min(min, pnl), 0);

        const checklist: ReadyToFundChecklist = {
          minTrades: {
            required: 50,
            current: stats.totalTrades,
            passed: stats.totalTrades >= 50,
          },
          winRate: {
            required: 45,
            current: stats.winRate,
            passed: stats.winRate >= 45,
          },
          profitFactor: {
            required: 1.3,
            current: stats.profitFactor,
            passed: stats.profitFactor >= 1.3,
          },
          maxSingleLoss: {
            required: -75,
            current: largestLoss,
            passed: largestLoss >= -75,
          },
          maxDailyLoss: {
            required: -150,
            current: worstDay,
            passed: worstDay >= -150,
          },
          allPassed: false,
        };

        checklist.allPassed = Object.values(checklist)
          .filter((v) => typeof v === 'object' && 'passed' in v)
          .every((v) => (v as { passed: boolean }).passed);

        return checklist;
      },

      getEquityCurve: () => {
        const sessions = get().sessions.sort(
          (a, b) => new Date(a.date).getTime() - new Date(b.date).getTime()
        );

        let runningEquity = 0;
        let peak = 0;

        return sessions.map((session) => {
          runningEquity += session.netPnl;
          if (runningEquity > peak) peak = runningEquity;
          const drawdown = peak - runningEquity;

          return {
            date: session.date,
            equity: runningEquity,
            drawdown,
          };
        });
      },

      // Import/Export
      exportData: () => {
        const { sessions, trades } = get();
        return JSON.stringify({ sessions, trades, exportedAt: new Date().toISOString() }, null, 2);
      },

      importData: (json) => {
        try {
          const data = JSON.parse(json);
          if (data.sessions && data.trades) {
            set({
              sessions: data.sessions,
              trades: data.trades,
            });
            return true;
          }
          return false;
        } catch {
          return false;
        }
      },

      clearAllData: () => {
        set({
          sessions: [],
          trades: [],
          currentSessionId: null,
        });
      },
    }),
    {
      name: 'hitthebid-journal',
      version: 1,
    }
  )
);
