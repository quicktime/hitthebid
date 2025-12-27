// Journal Types - Three-Element Trading Methodology

export type MarketState = 'balance' | 'imbalance';

export type LocationType =
  | 'lvn'
  | 'poc'
  | 'vah'
  | 'val'
  | 'vwap'
  | 'vwap_upper'
  | 'vwap_lower'
  | 'pdh'
  | 'pdl'
  | 'onh'
  | 'onl'
  | 'other';

export type AggressionType =
  | 'absorption'
  | 'delta_flip'
  | 'stacked_imbalance'
  | 'big_prints'
  | 'none';

export type SetupGrade = 'A' | 'B' | 'C';

export type TradeDirection = 'long' | 'short';

export type ExitType = 'target' | 'stop' | 'scratch' | 'manual' | 'trailing';

export type PremarketBias = 'bullish' | 'bearish' | 'neutral';

// Trading Session - represents one trading day
export interface TradingSession {
  id: string;
  date: string; // YYYY-MM-DD

  // Pre-market levels
  pdh: number | null;  // Prior Day High
  pdl: number | null;  // Prior Day Low
  pdc: number | null;  // Prior Day Close
  onh: number | null;  // Overnight High
  onl: number | null;  // Overnight Low
  poc: number | null;  // Point of Control
  vah: number | null;  // Value Area High
  val: number | null;  // Value Area Low
  lvnLevels: number[]; // LVN levels for the day

  // Daily setup
  premarketBias: PremarketBias;
  marketStateAtOpen: MarketState | null;
  dailyThesis: string;

  // Post-session review
  dailyReview: string;
  tomorrowFocus: string;
  lessonsLearned: string;

  // Computed stats (updated as trades are added)
  totalTrades: number;
  winners: number;
  losers: number;
  scratches: number;
  grossProfit: number;
  grossLoss: number;
  netPnl: number;

  // Prop firm tracking
  equityHigh: number;
  maxDrawdownFromHigh: number;

  createdAt: string;
  updatedAt: string;
}

// Individual Trade
export interface Trade {
  id: string;
  sessionId: string;
  tradeNumber: number; // Sequential number for the day

  // Timing
  entryTime: string;   // ISO timestamp
  exitTime: string | null;

  // Three Elements (pre-trade analysis)
  marketState: MarketState;
  locationType: LocationType;
  locationPrice: number;
  aggressionType: AggressionType;
  prismConfirmation: boolean; // Did PRISM confirm the signal?
  setupGrade: SetupGrade;

  // Execution plan
  direction: TradeDirection;
  entryPrice: number;
  stopPrice: number;
  targetPrice: number;
  positionSize: number; // Number of contracts
  plannedRR: number;    // Risk:Reward ratio

  // Result (filled after trade closes)
  exitPrice: number | null;
  exitType: ExitType | null;
  pnl: number | null;        // In dollars
  pnlPoints: number | null;  // In points
  actualRR: number | null;

  // Trade state
  isOpen: boolean;

  // Notes
  entryNotes: string;
  exitNotes: string;
  whatWorked: string;
  whatToImprove: string;
  screenshot: string | null; // URL to screenshot

  // Signal source (for auto-captured trades)
  signalSource: 'manual' | 'paper_trade' | 'backtest' | null;
  signalId: string | null;

  createdAt: string;
  updatedAt: string;
}

// Analytics calculations
export interface SessionStats {
  totalTrades: number;
  winRate: number;
  profitFactor: number;
  avgWinner: number;
  avgLoser: number;
  avgRR: number;
  netPnl: number;
  maxDrawdown: number;
  sharpeRatio: number;
}

export interface BreakdownStats {
  count: number;
  wins: number;
  losses: number;
  winRate: number;
  netPnl: number;
  avgPnl: number;
}

// Ready to Fund checklist (for prop firm evaluation)
export interface ReadyToFundChecklist {
  minTrades: { required: number; current: number; passed: boolean };
  winRate: { required: number; current: number; passed: boolean };
  profitFactor: { required: number; current: number; passed: boolean };
  maxSingleLoss: { required: number; current: number; passed: boolean };
  maxDailyLoss: { required: number; current: number; passed: boolean };
  allPassed: boolean;
}

// Form types for creating/editing
export interface NewSessionForm {
  date: string;
  pdh: string;
  pdl: string;
  pdc: string;
  onh: string;
  onl: string;
  poc: string;
  vah: string;
  val: string;
  lvnLevels: string; // Comma-separated
  premarketBias: PremarketBias;
  dailyThesis: string;
}

export interface NewTradeForm {
  marketState: MarketState;
  locationType: LocationType;
  locationPrice: string;
  aggressionType: AggressionType;
  prismConfirmation: boolean;
  setupGrade: SetupGrade;
  direction: TradeDirection;
  entryPrice: string;
  stopPrice: string;
  targetPrice: string;
  positionSize: string;
  entryNotes: string;
}

export interface CloseTradeForm {
  exitPrice: string;
  exitType: ExitType;
  exitNotes: string;
  whatWorked: string;
  whatToImprove: string;
}
