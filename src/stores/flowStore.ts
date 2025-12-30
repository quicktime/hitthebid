import { create } from 'zustand';
import { RustWebSocket, WsMessage, ReplayStatus } from '../websocket';

// Types
export interface Bubble {
  id: string;
  symbol: string;
  price: number;
  size: number;
  side: 'buy' | 'sell';
  timestamp: number;
  x: number;
  opacity: number;
  isSignificantImbalance?: boolean;
}

export interface CVDPoint {
  timestamp: number;
  value: number;
  x: number;
}

export interface ZeroCross {
  timestamp: number;
  direction: 'bullish' | 'bearish';
  x: number;
  price?: number;
}

export interface AbsorptionAlert {
  timestamp: number;
  price: number;
  absorptionType: 'buying' | 'selling';
  delta: number;
  strength: 'weak' | 'medium' | 'strong' | 'defended';
  eventCount: number;
  totalAbsorbed: number;
  atKeyLevel: boolean;
  againstTrend: boolean;
  x: number;
}

export interface AbsorptionZone {
  price: number;
  absorptionType: 'buying' | 'selling';
  totalAbsorbed: number;
  eventCount: number;
  strength: 'weak' | 'medium' | 'strong' | 'defended';
  atPoc: boolean;
  atVah: boolean;
  atVal: boolean;
  againstTrend: boolean;
}

export interface VolumeProfileLevel {
  price: number;
  buyVolume: number;
  sellVolume: number;
  totalVolume: number;
}

export interface StackedImbalance {
  timestamp: number;
  side: 'buy' | 'sell';
  levelCount: number;
  priceHigh: number;
  priceLow: number;
  totalImbalance: number;
  x: number;
}

export interface ConfluenceEvent {
  timestamp: number;
  price: number;
  direction: 'bullish' | 'bearish';
  score: number;
  signals: string[];
  x: number;
}

export interface TradingSignal {
  timestamp: number;
  signalType: 'entry' | 'exit' | 'stop_update' | 'flatten';
  direction: 'long' | 'short' | '';
  price: number;
  stop?: number;
  target?: number;
  pnlPoints?: number;
  reason?: string;
  x: number;
}

export interface SignalStats {
  count: number;
  bullishCount: number;
  bearishCount: number;
  wins: number;
  losses: number;
  avgMove1m: number;
  avgMove5m: number;
  winRate: number;
}

export interface SessionStats {
  sessionStart: number;
  deltaFlips: SignalStats;
  absorptions: SignalStats;
  stackedImbalances: SignalStats;
  confluences: SignalStats;
  currentPrice: number;
  sessionHigh: number;
  sessionLow: number;
  totalVolume: number;
}

interface FlowState {
  // Connection
  isConnected: boolean;
  error: string | null;
  serverMode: 'live' | 'demo' | 'replay';
  connectedSymbols: string[];

  // Price data
  lastPrice: number | null;
  priceRange: { min: number; max: number } | null;

  // CVD
  currentCVD: number;
  cvdHistory: CVDPoint[];
  cvdRange: { min: number; max: number };
  zeroCrosses: ZeroCross[];
  cvdStartTime: number;

  // Bubbles
  bubbles: Bubble[];
  volumeProfile: Map<number, VolumeProfileLevel>;

  // Signals
  absorptionAlerts: AbsorptionAlert[];
  absorptionZones: AbsorptionZone[];
  stackedImbalances: StackedImbalance[];
  confluenceEvents: ConfluenceEvent[];

  // Trading signals
  tradingSignals: TradingSignal[];
  activeTradingSignal: TradingSignal | null;

  // Session
  sessionStats: SessionStats | null;

  // Replay
  replayStatus: ReplayStatus | null;
  isPaused: boolean;

  // Internal refs (for CVD calculation)
  _cvdBaseline: number;
  _lastRawCvd: number;
  _prevAdjustedCvd: number;
  _ws: RustWebSocket | null;

  // Actions
  connect: () => void;
  disconnect: () => void;
  resetCVD: () => void;
  pause: () => void;
  resume: () => void;
  togglePause: () => void;
  setReplaySpeed: (speed: number) => void;
  setMinSize: (size: number) => void;
  clearBubbles: () => void;

  // Animation methods
  animateFrame: (movement: number) => void;
  cleanupOldItems: (maxAgeMs: number) => void;
  clearError: () => void;

  // Message handlers (called internally)
  handleMessage: (message: WsMessage) => void;

  // Getters
  getFilteredBubbles: (selectedSymbol: string) => Bubble[];
}

export const useFlowStore = create<FlowState>()((set, get) => ({
  // Initial state
  isConnected: false,
  error: null,
  serverMode: 'live',
  connectedSymbols: [],
  lastPrice: null,
  priceRange: null,
  currentCVD: 0,
  cvdHistory: [],
  cvdRange: { min: 0, max: 0 },
  zeroCrosses: [],
  cvdStartTime: Date.now(),
  bubbles: [],
  volumeProfile: new Map(),
  absorptionAlerts: [],
  absorptionZones: [],
  stackedImbalances: [],
  confluenceEvents: [],
  tradingSignals: [],
  activeTradingSignal: null,
  sessionStats: null,
  replayStatus: null,
  isPaused: false,
  _cvdBaseline: 0,
  _lastRawCvd: 0,
  _prevAdjustedCvd: 0,
  _ws: null,

  // Actions
  connect: () => {
    const ws = new RustWebSocket();

    ws.onConnect(() => {
      set({ isConnected: true, error: null });
      console.log('Connected to Rust backend');
    });

    ws.onDisconnect(() => {
      set({ isConnected: false });
      console.log('Disconnected from Rust backend');
    });

    ws.onReconnecting((attempt: number) => {
      set({ error: `Reconnecting... (attempt ${attempt})` });
    });

    ws.onMessage((message: WsMessage) => {
      get().handleMessage(message);
    });

    ws.connect();
    set({ _ws: ws });
  },

  disconnect: () => {
    const ws = get()._ws;
    if (ws) {
      ws.disconnect();
      set({ _ws: null, isConnected: false });
    }
  },

  resetCVD: () => {
    const lastRawCvd = get()._lastRawCvd;
    set({
      _cvdBaseline: lastRawCvd,
      _prevAdjustedCvd: 0,
      currentCVD: 0,
      cvdHistory: [],
      cvdRange: { min: 0, max: 0 },
      zeroCrosses: [],
      cvdStartTime: Date.now(),
    });
  },

  pause: () => {
    const ws = get()._ws;
    if (ws) {
      ws.replayPause();
      set({ isPaused: true });
    }
  },

  resume: () => {
    const ws = get()._ws;
    if (ws) {
      ws.replayResume();
      set({ isPaused: false });
    }
  },

  setReplaySpeed: (speed: number) => {
    const ws = get()._ws;
    if (ws) {
      ws.setReplaySpeed(speed);
    }
  },

  setMinSize: (size: number) => {
    const ws = get()._ws;
    if (ws) {
      ws.setMinSize(size);
    }
  },

  clearBubbles: () => {
    set({ bubbles: [] });
  },

  togglePause: () => {
    const state = get();
    if (state.isPaused) {
      get().resume();
    } else {
      get().pause();
    }
  },

  animateFrame: (movement: number) => {
    const state = get();
    set({
      bubbles: state.bubbles.map((bubble) => ({
        ...bubble,
        x: bubble.x - movement,
        opacity: 1,
      })),
      cvdHistory: state.cvdHistory.map((point) => ({
        ...point,
        x: point.x - movement,
      })),
      zeroCrosses: state.zeroCrosses.map((cross) => ({
        ...cross,
        x: cross.x - movement,
      })),
      absorptionAlerts: state.absorptionAlerts.map((alert) => ({
        ...alert,
        x: alert.x - movement,
      })),
      stackedImbalances: state.stackedImbalances.map((stacked) => ({
        ...stacked,
        x: stacked.x - movement,
      })),
      confluenceEvents: state.confluenceEvents.map((conf) => ({
        ...conf,
        x: conf.x - movement,
      })),
    });
  },

  cleanupOldItems: (maxAgeMs: number) => {
    const state = get();
    const now = Date.now();
    set({
      bubbles: state.bubbles.filter((b) => now - b.timestamp < maxAgeMs),
      cvdHistory: state.cvdHistory.filter((p) => now - p.timestamp < maxAgeMs),
      zeroCrosses: state.zeroCrosses.filter((c) => now - c.timestamp < maxAgeMs),
      absorptionAlerts: state.absorptionAlerts.filter((a) => now - a.timestamp < maxAgeMs),
      stackedImbalances: state.stackedImbalances.filter((s) => now - s.timestamp < maxAgeMs),
      confluenceEvents: state.confluenceEvents.filter((c) => now - c.timestamp < maxAgeMs),
      tradingSignals: state.tradingSignals.filter((s) => now - s.timestamp < maxAgeMs),
    });
  },

  clearError: () => {
    set({ error: null });
  },

  handleMessage: (message: WsMessage) => {
    const state = get();


    switch (message.type) {
      case 'Bubble': {
        const MAX_BUBBLES = 80; // Limit for performance

        const bubble: Bubble = {
          id: message.id,
          symbol: message.symbol,
          price: message.price,
          size: message.size,
          side: message.side,
          timestamp: message.timestamp,
          x: message.x,
          opacity: message.opacity,
          isSignificantImbalance: message.isSignificantImbalance,
        };

        // Add new bubble and keep only the most recent MAX_BUBBLES
        let newBubbles = [...state.bubbles, bubble];
        if (newBubbles.length > MAX_BUBBLES) {
          newBubbles = newBubbles.slice(-MAX_BUBBLES);
        }

        const newPriceRange = state.priceRange
          ? {
              min: Math.min(state.priceRange.min, bubble.price - (state.priceRange.max - state.priceRange.min) * 0.1),
              max: Math.max(state.priceRange.max, bubble.price + (state.priceRange.max - state.priceRange.min) * 0.1),
            }
          : { min: bubble.price - 10, max: bubble.price + 10 };

        set({
          bubbles: newBubbles,
          lastPrice: bubble.price,
          priceRange: newPriceRange,
        });
        break;
      }

      case 'CVDPoint': {
        const rawCvd = message.value;
        const adjustedCvd = rawCvd - state._cvdBaseline;

        const cvdPoint: CVDPoint = {
          timestamp: message.timestamp,
          value: adjustedCvd,
          x: message.x,
        };

        // Zero-cross detection
        const prevCvd = state._prevAdjustedCvd;
        const prevSign = Math.sign(prevCvd);
        const newSign = Math.sign(adjustedCvd);
        let newZeroCrosses = state.zeroCrosses;

        if (prevSign !== 0 && newSign !== 0 && prevSign !== newSign && Math.abs(prevCvd) >= 300) {
          const direction = adjustedCvd > 0 ? 'bullish' : 'bearish';
          console.log(`CVD ZERO CROSS: ${direction.toUpperCase()}`);

          newZeroCrosses = [
            ...state.zeroCrosses,
            {
              timestamp: message.timestamp,
              direction,
              x: message.x,
              price: state.lastPrice ?? undefined,
            },
          ];
        }

        const MAX_CVD_POINTS = 200; // Limit for performance
        let newCvdHistory = [...state.cvdHistory, cvdPoint];
        if (newCvdHistory.length > MAX_CVD_POINTS) {
          newCvdHistory = newCvdHistory.slice(-MAX_CVD_POINTS);
        }

        set({
          cvdHistory: newCvdHistory,
          currentCVD: adjustedCvd,
          cvdRange: {
            min: Math.min(state.cvdRange.min, adjustedCvd),
            max: Math.max(state.cvdRange.max, adjustedCvd),
          },
          zeroCrosses: newZeroCrosses,
          _lastRawCvd: rawCvd,
          _prevAdjustedCvd: adjustedCvd,
        });
        break;
      }

      case 'VolumeProfile': {
        const newProfile = new Map<number, VolumeProfileLevel>();
        for (const level of message.levels) {
          newProfile.set(level.price, level);
        }
        set({ volumeProfile: newProfile });
        break;
      }

      case 'Absorption': {
        const alert: AbsorptionAlert = {
          timestamp: message.timestamp,
          price: message.price,
          absorptionType: message.absorptionType,
          delta: message.delta,
          strength: message.strength,
          eventCount: message.eventCount,
          totalAbsorbed: message.totalAbsorbed,
          atKeyLevel: message.atKeyLevel,
          againstTrend: message.againstTrend,
          x: message.x,
        };
        set({ absorptionAlerts: [...state.absorptionAlerts, alert] });
        break;
      }

      case 'AbsorptionZones': {
        set({ absorptionZones: message.zones });
        break;
      }

      case 'StackedImbalance': {
        const imbalance: StackedImbalance = {
          timestamp: message.timestamp,
          side: message.side,
          levelCount: message.levelCount,
          priceHigh: message.priceHigh,
          priceLow: message.priceLow,
          totalImbalance: message.totalImbalance,
          x: message.x,
        };
        set({ stackedImbalances: [...state.stackedImbalances, imbalance] });
        break;
      }

      case 'Confluence': {
        const event: ConfluenceEvent = {
          timestamp: message.timestamp,
          price: message.price,
          direction: message.direction,
          score: message.score,
          signals: message.signals,
          x: message.x,
        };
        set({ confluenceEvents: [...state.confluenceEvents, event] });
        break;
      }

      case 'TradingSignal': {
        const signal: TradingSignal = {
          timestamp: message.timestamp,
          signalType: message.signalType,
          direction: message.direction,
          price: message.price,
          stop: message.stop,
          target: message.target,
          pnlPoints: message.pnlPoints,
          reason: message.reason,
          x: message.x,
        };

        // Keep max 50 trading signals
        const MAX_SIGNALS = 50;
        let newSignals = [...state.tradingSignals, signal];
        if (newSignals.length > MAX_SIGNALS) {
          newSignals = newSignals.slice(-MAX_SIGNALS);
        }

        // Track active position
        if (signal.signalType === 'entry') {
          set({
            tradingSignals: newSignals,
            activeTradingSignal: signal,
          });
        } else if (signal.signalType === 'exit' || signal.signalType === 'flatten') {
          set({
            tradingSignals: newSignals,
            activeTradingSignal: null,
          });
        } else if (signal.signalType === 'stop_update' && state.activeTradingSignal) {
          // Update the active signal's stop
          const updated = { ...state.activeTradingSignal, stop: signal.stop };
          set({
            tradingSignals: newSignals,
            activeTradingSignal: updated,
          });
        } else {
          set({ tradingSignals: newSignals });
        }
        break;
      }

      case 'SessionStats': {
        set({ sessionStats: message as SessionStats });
        break;
      }

      case 'Connected': {
        set({
          serverMode: message.mode as 'live' | 'demo' | 'replay',
          connectedSymbols: message.symbols,
        });
        break;
      }

      case 'ReplayStatus': {
        set({ replayStatus: message as ReplayStatus });
        break;
      }

      case 'Error': {
        set({ error: message.message });
        break;
      }
    }
  },

  getFilteredBubbles: (_selectedSymbol: string) => {
    // Always return all bubbles since we only trade NQ now
    return get().bubbles;
  },
}));
