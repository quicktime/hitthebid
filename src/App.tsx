import { useEffect, useRef, useState, useCallback } from 'react';
import { TradovateConnection, Trade } from './tradovate';
import { DemoDataGenerator } from './demoData';
import { BubbleRenderer } from './BubbleRenderer';
import './App.css';

interface Bubble {
  id: string;
  price: number;
  size: number;
  side: 'buy' | 'sell';
  timestamp: number;
  x: number;
  opacity: number;
  isStackedImbalance?: boolean; // Part of 3+ consecutive same-side trades
}

interface CVDPoint {
  timestamp: number;
  value: number;
  x: number;
}

interface ZeroCross {
  timestamp: number;
  direction: 'bullish' | 'bearish'; // Crossing up = bullish, down = bearish
  x: number;
  price?: number;
}

interface Divergence {
  type: 'bullish' | 'bearish';
  timestamp: number;
  priceLevel: number;
  cvdValue: number;
}

// Update these for current front-month contracts
// H=Mar, M=Jun, U=Sep, Z=Dec
const SYMBOLS = ['NQH5', 'ESH5'] as const;
type Symbol = typeof SYMBOLS[number];

// Audio alert function for zero crosses
function playAlertSound(direction: 'bullish' | 'bearish') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    // Bullish = higher pitch, Bearish = lower pitch
    oscillator.frequency.value = direction === 'bullish' ? 800 : 400;
    oscillator.type = 'sine';

    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.3);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.3);
  } catch (e) {
    console.log('Audio not supported', e);
  }
}

function App() {
  const [isConnected, setIsConnected] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [isDemoMode, setIsDemoMode] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedSymbol, setSelectedSymbol] = useState<Symbol>('NQH5');
  const [bubbles, setBubbles] = useState<Bubble[]>([]);
  const [lastPrice, setLastPrice] = useState<number | null>(null);
  const [priceRange, setPriceRange] = useState<{ min: number; max: number } | null>(null);
  const [minBubbleSize, setMinBubbleSize] = useState(1); // Minimum contracts to show
  const [cvdHistory, setCvdHistory] = useState<CVDPoint[]>([]);
  const [currentCVD, setCurrentCVD] = useState(0);
  const [cvdRange, setCvdRange] = useState<{ min: number; max: number }>({ min: 0, max: 0 });
  const [zeroCrosses, setZeroCrosses] = useState<ZeroCross[]>([]);
  const [lastCVDSign, setLastCVDSign] = useState<number>(0);
  const [cvdFlashAlert, setCvdFlashAlert] = useState<'bullish' | 'bearish' | null>(null);
  const [showCvdBadge, setShowCvdBadge] = useState<'bullish' | 'bearish' | null>(null);
  const [divergences, setDivergences] = useState<Divergence[]>([]);

  // CVD Reset State
  const [cvdStartTime, setCvdStartTime] = useState<number>(Date.now());
  const [cvdMinThreshold, setCvdMinThreshold] = useState(300); // Min CVD magnitude for alerts
  const [isSoundEnabled, setIsSoundEnabled] = useState(true); // Sound toggle for alerts
  const [totalBuyVolume, setTotalBuyVolume] = useState(0); // Total buy volume
  const [totalSellVolume, setTotalSellVolume] = useState(0); // Total sell volume
  const [credentials, setCredentials] = useState({
    username: '',
    password: '',
    appId: '',
    appVersion: '1.0',
    cid: '',
    sec: ''
  });
  
  const connectionRef = useRef<TradovateConnection | null>(null);
  const demoRef = useRef<DemoDataGenerator | null>(null);
  const bubbleIdRef = useRef(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const priceHighsRef = useRef<Array<{ price: number; timestamp: number }>>([]);
  const priceLowsRef = useRef<Array<{ price: number; timestamp: number }>>([]);
  const cvdHighsRef = useRef<Array<{ cvd: number; timestamp: number }>>([]);
  const cvdLowsRef = useRef<Array<{ cvd: number; timestamp: number }>>([]);
  const sessionResetTimerRef = useRef<NodeJS.Timeout | null>(null);
  const [isPaused, setIsPaused] = useState(false);
  const [selectedBubble, setSelectedBubble] = useState<Bubble | null>(null);
  const [clickPosition, setClickPosition] = useState<{ x: number; y: number } | null>(null);
  const [showShortcutsHelp, setShowShortcutsHelp] = useState(false);

  // Reset CVD function
  const resetCVD = useCallback(() => {
    setCurrentCVD(0);
    setCvdHistory([]);
    setCvdRange({ min: 0, max: 0 });
    setZeroCrosses([]);
    setLastCVDSign(0);
    setCvdStartTime(Date.now());
    setTotalBuyVolume(0);
    setTotalSellVolume(0);
    console.log('🔄 CVD RESET - Starting fresh');
  }, []);

  // Log threshold changes
  useEffect(() => {
    console.log(`⚙️ CVD Threshold updated: ${cvdMinThreshold}`);
  }, [cvdMinThreshold]);

  // Check if zero-cross should trigger alert
  const shouldTriggerZeroCrossAlert = useCallback((prevCVD: number, newCVD: number) => {
    // Check if crossing zero
    const prevSign = Math.sign(prevCVD);
    const newSign = Math.sign(newCVD);
    if (prevSign === 0 || newSign === 0 || prevSign === newSign) {
      return false; // Not a zero cross
    }

    // Check minimum threshold - prevents noise
    const prevMagnitude = Math.abs(prevCVD);
    if (prevMagnitude < cvdMinThreshold) {
      console.log(`⚠️ CVD cross ignored: ${prevCVD.toFixed(0)} → ${newCVD.toFixed(0)} (threshold: ${cvdMinThreshold})`);
      return false;
    }

    console.log(`✅ CVD cross threshold check passed! (${prevMagnitude.toFixed(0)} >= ${cvdMinThreshold})`);
    return true;
  }, [cvdMinThreshold]);

  // Handle incoming trades
  const handleTrade = useCallback((trade: Trade) => {
    if (trade.size < minBubbleSize) return;

    // Track volume totals
    if (trade.side === 'buy') {
      setTotalBuyVolume(prev => prev + trade.size);
    } else {
      setTotalSellVolume(prev => prev + trade.size);
    }

    // Update CVD (Cumulative Volume Delta)
    const delta = trade.side === 'buy' ? trade.size : -trade.size;

    // Debug: Log correlation every 10th trade
    if (bubbleIdRef.current % 10 === 0) {
      console.log(`Trade #${bubbleIdRef.current}: ${trade.side.toUpperCase()} ${trade.size} contracts → Delta: ${delta > 0 ? '+' : ''}${delta}`);
    }

    setCurrentCVD(prev => {
      const newCVD = prev + delta;

      // Debug: Log CVD updates
      if (bubbleIdRef.current % 10 === 0) {
        console.log(`CVD: ${prev.toFixed(0)} → ${newCVD.toFixed(0)} (${newCVD > prev ? '↑' : '↓'})`);
      }

      // ZERO-CROSS DETECTION with thresholds and warm-up check
      if (shouldTriggerZeroCrossAlert(prev, newCVD)) {
        // CVD crossed zero!
        const direction = newCVD > 0 ? 'bullish' : 'bearish';

        console.log(`🚨 CVD ZERO CROSS: ${direction.toUpperCase()} (was: ${prev.toFixed(0)}, now: ${newCVD.toFixed(0)}) 🚨`);

        // Add zero cross marker
        setZeroCrosses(prevCrosses => {
          const newCross: ZeroCross = {
            timestamp: Date.now(),
            direction,
            x: 0.92,
            price: trade.price
          };
          const updated = [...prevCrosses, newCross];
          return updated.length > 100 ? updated.slice(-100) : updated;
        });

        // Trigger flash alert
        setCvdFlashAlert(direction);
        setTimeout(() => setCvdFlashAlert(null), 500);

        // Show badge for 3 seconds
        setShowCvdBadge(direction);
        setTimeout(() => setShowCvdBadge(null), 3000);

        // Play audio alert (if enabled)
        if (isSoundEnabled) {
          playAlertSound(direction);
        }
      }

      setLastCVDSign(Math.sign(newCVD));

      // Add to CVD history
      setCvdHistory(prevHistory => {
        const newPoint: CVDPoint = {
          timestamp: Date.now(),
          value: newCVD,
          x: 0.92 // Start with same position as bubbles
        };
        const updated = [...prevHistory, newPoint];
        // Keep last 1000 CVD points
        return updated.length > 1000 ? updated.slice(-1000) : updated;
      });

      // Update CVD range for scaling
      setCvdRange(prevRange => ({
        min: Math.min(prevRange.min, newCVD),
        max: Math.max(prevRange.max, newCVD)
      }));

      return newCVD;
    });

    // Detect stacked imbalances (3+ consecutive same-side trades within price range)
    setBubbles(prev => {
      // Check last 2 bubbles for stacked pattern
      const recentBubbles = prev.slice(-2);
      const isStackedImbalance =
        recentBubbles.length === 2 &&
        recentBubbles.every(b => b.side === trade.side) &&
        recentBubbles.every(b => Math.abs(b.price - trade.price) < 5); // Within 5 points

      const newBubble: Bubble = {
        id: `bubble-${bubbleIdRef.current++}`,
        price: trade.price,
        size: trade.size,
        side: trade.side,
        timestamp: Date.now(),
        x: 0.92, // Start slightly left of right edge (inside screen)
        opacity: 1,
        isStackedImbalance
      };

      // Mark previous bubbles as stacked if pattern detected
      let updated = [...prev, newBubble];
      if (isStackedImbalance) {
        // Mark the last 2 bubbles as stacked as well
        for (let i = updated.length - 3; i < updated.length - 1; i++) {
          if (i >= 0) {
            updated[i] = { ...updated[i], isStackedImbalance: true };
          }
        }
      }

      // Keep last 1000 bubbles max
      if (updated.length > 1000) {
        updated = updated.slice(-1000);
      }
      return updated;
    });

    setLastPrice(trade.price);

    // Update price range
    setPriceRange(prev => {
      if (!prev) {
        return { min: trade.price - 10, max: trade.price + 10 };
      }
      const padding = (prev.max - prev.min) * 0.1;
      return {
        min: Math.min(prev.min, trade.price - padding),
        max: Math.max(prev.max, trade.price + padding)
      };
    });
  }, [minBubbleSize, shouldTriggerZeroCrossAlert, isSoundEnabled]);

  // Session-based auto-reset timer (9:30 AM, 12:00 PM, 3:00 PM ET)
  useEffect(() => {
    if (!isConnected) return;

    const scheduleNextReset = () => {
      const now = new Date();
      const et = new Date(now.toLocaleString('en-US', { timeZone: 'America/New_York' }));

      // Session reset times (ET): 9:30 AM, 12:00 PM, 3:00 PM
      const resetTimes = [
        { hour: 9, minute: 30, label: 'Market Open' },
        { hour: 12, minute: 0, label: 'Midday' },
        { hour: 15, minute: 0, label: 'Power Hour' }
      ];

      let nextResetDate: Date | null = null;
      let nextLabel = '';

      for (const time of resetTimes) {
        const resetDate = new Date(et);
        resetDate.setHours(time.hour, time.minute, 0, 0);

        if (resetDate > et) {
          nextResetDate = resetDate;
          nextLabel = time.label;
          break;
        }
      }

      // If no reset today, schedule for tomorrow's market open
      if (!nextResetDate) {
        nextResetDate = new Date(et);
        nextResetDate.setDate(nextResetDate.getDate() + 1);
        nextResetDate.setHours(9, 30, 0, 0);
        nextLabel = 'Market Open';
      }

      const msUntilReset = nextResetDate.getTime() - et.getTime();

      console.log(`⏰ Next CVD reset: ${nextLabel} at ${nextResetDate.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', hour12: true })} ET (in ${(msUntilReset / 60000).toFixed(0)} minutes)`);

      // Clear existing timer
      if (sessionResetTimerRef.current) {
        clearTimeout(sessionResetTimerRef.current);
      }

      // Set new timer
      sessionResetTimerRef.current = setTimeout(() => {
        console.log(`🔔 Session Reset: ${nextLabel}`);
        resetCVD();
        scheduleNextReset(); // Schedule next reset
      }, msUntilReset);
    };

    scheduleNextReset();

    return () => {
      if (sessionResetTimerRef.current) {
        clearTimeout(sessionResetTimerRef.current);
      }
    };
  }, [isConnected, resetCVD]);

  // Export screenshot
  const exportScreenshot = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    try {
      // Convert canvas to blob
      canvas.toBlob((blob) => {
        if (!blob) return;

        // Create download link
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
        link.download = `flow-orderflow-${timestamp}.png`;
        link.href = url;
        link.click();

        // Cleanup
        URL.revokeObjectURL(url);
        console.log('📸 Screenshot exported');
      }, 'image/png');
    } catch (err) {
      console.error('Failed to export screenshot:', err);
    }
  }, []);

  // Handle canvas click to show bubble info
  const handleCanvasClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || !priceRange) return;

    const rect = canvas.getBoundingClientRect();
    const clickX = e.clientX - rect.left;
    const clickY = e.clientY - rect.top;

    // Convert click position to normalized coordinates
    const normalizedX = clickX / rect.width;
    const normalizedY = clickY / rect.height;

    // Find bubble at click position
    const MIN_BUBBLE_RADIUS = 4;
    const MAX_BUBBLE_RADIUS = 60;
    const SIZE_SCALE_FACTOR = 2;
    const priceSpan = priceRange.max - priceRange.min;

    let clickedBubble: Bubble | null = null;
    let minDistance = Infinity;

    // Check bubbles in reverse order (newest first)
    for (let i = bubbles.length - 1; i >= 0; i--) {
      const bubble = bubbles[i];
      const bubbleX = bubble.x;
      const bubbleY = 1 - ((bubble.price - priceRange.min) / priceSpan);
      const radius = Math.min(
        MAX_BUBBLE_RADIUS,
        Math.max(MIN_BUBBLE_RADIUS, Math.sqrt(bubble.size) * SIZE_SCALE_FACTOR)
      ) / rect.width; // Normalize radius

      const dx = normalizedX - bubbleX;
      const dy = normalizedY - bubbleY;
      const distance = Math.sqrt(dx * dx + dy * dy);

      if (distance <= radius && distance < minDistance) {
        clickedBubble = bubble;
        minDistance = distance;
      }
    }

    if (clickedBubble) {
      setSelectedBubble(clickedBubble);
      setClickPosition({ x: clickX, y: clickY });
    } else {
      setSelectedBubble(null);
      setClickPosition(null);
    }
  }, [bubbles, priceRange]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyPress = (e: KeyboardEvent) => {
      // Ignore if typing in input field
      if ((e.target as HTMLElement).tagName === 'INPUT') return;

      const key = e.key.toLowerCase();

      // Special keys that work even with modals open
      if (key === 'escape') {
        if (showShortcutsHelp) {
          setShowShortcutsHelp(false);
          return;
        }
        if (selectedBubble) {
          setSelectedBubble(null);
          setClickPosition(null);
          return;
        }
      }

      if (key === '?' || (e.shiftKey && key === '/')) {
        setShowShortcutsHelp(prev => !prev);
        return;
      }

      // Don't process other shortcuts if modal is open
      if (showShortcutsHelp) return;

      switch (key) {
        case 'r':
          // Reset CVD
          resetCVD();
          console.log('⌨️ Keyboard: CVD Reset (R)');
          break;
        case ' ':
          // Toggle pause
          e.preventDefault(); // Prevent page scroll
          setIsPaused(prev => {
            console.log(`⌨️ Keyboard: ${!prev ? 'Paused' : 'Resumed'} (Space)`);
            return !prev;
          });
          break;
        case 'c':
          // Clear all bubbles
          setBubbles([]);
          console.log('⌨️ Keyboard: Cleared bubbles (C)');
          break;
        case 'm':
          // Toggle sound
          setIsSoundEnabled(prev => {
            console.log(`⌨️ Keyboard: Sound ${!prev ? 'Enabled' : 'Muted'} (M)`);
            return !prev;
          });
          break;
        case 's':
          // Export screenshot
          exportScreenshot();
          break;
        default:
          // Check for number keys 1-9 to set min bubble size
          const num = parseInt(key);
          if (!isNaN(num) && num >= 1 && num <= 9) {
            const size = num * 10;
            setMinBubbleSize(size);
            console.log(`⌨️ Keyboard: Min size set to ${size} (${key})`);
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyPress);
    return () => window.removeEventListener('keydown', handleKeyPress);
  }, [resetCVD, exportScreenshot, showShortcutsHelp, selectedBubble]);

  // Animation loop to move bubbles left and fade out
  // Settings for faster panning: ~45 seconds to traverse screen
  useEffect(() => {
    const interval = setInterval(() => {
      // Skip updates if paused
      if (isPaused) return;

      setBubbles(prev => {
        return prev
          .map(bubble => ({
            ...bubble,
            x: bubble.x - 0.00040, // Move left (2x faster)
            opacity: Math.max(0, bubble.opacity - 0.00003) // Slightly faster fade
          }))
          .filter(bubble => bubble.x > -0.1 && bubble.opacity > 0);
      });

      // Also move CVD points
      setCvdHistory(prev => {
        return prev
          .map(point => ({
            ...point,
            x: point.x - 0.00040 // Same speed as bubbles
          }))
          .filter(point => point.x > -0.1);
      });

      // Move zero cross markers
      setZeroCrosses(prev => {
        return prev
          .map(cross => ({
            ...cross,
            x: cross.x - 0.00040
          }))
          .filter(cross => cross.x > -0.1);
      });
    }, 16); // ~60fps

    return () => clearInterval(interval);
  }, [isPaused]);

  const connect = async () => {
    setIsConnecting(true);
    setError(null);
    
    try {
      connectionRef.current = new TradovateConnection(credentials);
      await connectionRef.current.connect();
      
      connectionRef.current.onTrade(handleTrade);
      connectionRef.current.onDisconnect(() => {
        setIsConnected(false);
        setError('Disconnected from Tradovate');
      });
      
      await connectionRef.current.subscribeToSymbol(selectedSymbol);
      setIsConnected(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Connection failed');
    } finally {
      setIsConnecting(false);
    }
  };

  const startDemo = () => {
    demoRef.current = new DemoDataGenerator(selectedSymbol);
    demoRef.current.onTrade(handleTrade);
    demoRef.current.start();
    setIsDemoMode(true);
    setIsConnected(true);
  };

  // Re-subscribe trade handler when it changes (fixes threshold update bug)
  useEffect(() => {
    if (demoRef.current && isDemoMode) {
      console.log('🔄 Re-subscribing demo trade handler with updated dependencies');
      demoRef.current.onTrade(handleTrade);
    }
    if (connectionRef.current && isConnected && !isDemoMode) {
      console.log('🔄 Re-subscribing live trade handler with updated dependencies');
      connectionRef.current.onTrade(handleTrade);
    }
  }, [handleTrade, isDemoMode, isConnected]);

  // Handle pause/resume for demo mode
  useEffect(() => {
    if (demoRef.current && isDemoMode) {
      if (isPaused) {
        demoRef.current.stop();
      } else {
        demoRef.current.start();
      }
    }
  }, [isPaused, isDemoMode]);

  const disconnect = () => {
    connectionRef.current?.disconnect();
    demoRef.current?.stop();
    setIsConnected(false);
    setIsDemoMode(false);
    setBubbles([]);
    setPriceRange(null);
  };

  // Switch symbols
  const handleSymbolChange = async (symbol: Symbol) => {
    if (connectionRef.current && isConnected && !isDemoMode) {
      await connectionRef.current.unsubscribeFromSymbol(selectedSymbol);
      await connectionRef.current.subscribeToSymbol(symbol);
    }
    if (demoRef.current && isDemoMode) {
      demoRef.current.setSymbol(symbol);
    }
    setSelectedSymbol(symbol);
    setBubbles([]);
    setPriceRange(null);
  };

  return (
    <div className="app">
      <header className="header">
        <div className="header-left">
          <h1 className="logo">
            <span className="logo-icon">◉</span>
            FLOW
          </h1>
          <div className="symbol-selector">
            {SYMBOLS.map(sym => (
              <button
                key={sym}
                className={`symbol-btn ${selectedSymbol === sym ? 'active' : ''}`}
                onClick={() => handleSymbolChange(sym)}
                disabled={!isConnected}
              >
                {sym}
              </button>
            ))}
          </div>
        </div>
        
        <div className="header-center">
          {lastPrice && (
            <div className="last-price">
              <span className="price-label">LAST</span>
              <span className="price-value">{lastPrice.toFixed(2)}</span>
            </div>
          )}
        </div>

        <div className="header-right">
          {/* CVD Widget - Always visible */}
          {isConnected && (
            <>
              <div className={`cvd-widget ${currentCVD >= 0 ? 'bullish' : 'bearish'}`}>
                <label>CVD</label>
                <div className="cvd-value">
                  {currentCVD > 0 ? '+' : ''}{currentCVD.toFixed(0)}
                </div>
                <div className="cvd-direction">
                  {currentCVD >= 0 ? '↗ BULLISH' : '↘ BEARISH'}
                </div>
                <div className="cvd-age">
                  Since {new Date(cvdStartTime).toLocaleTimeString('en-US', {
                    hour: 'numeric',
                    minute: '2-digit',
                    hour12: true
                  })}
                </div>
                <div className="cvd-threshold-display">
                  Alert threshold: ±{cvdMinThreshold}
                </div>
              </div>
              <button className="reset-cvd-btn" onClick={resetCVD} title="Reset CVD to zero">
                🔄
              </button>
              <div className="threshold-control">
                <label>THRESHOLD</label>
                <input
                  type="number"
                  min="0"
                  step="50"
                  value={cvdMinThreshold}
                  onChange={(e) => setCvdMinThreshold(Math.max(0, parseInt(e.target.value) || 0))}
                  title="Minimum CVD magnitude for zero-cross alerts"
                />
              </div>
              <button
                className={`sound-toggle-btn ${isSoundEnabled ? 'enabled' : 'disabled'}`}
                onClick={() => setIsSoundEnabled(!isSoundEnabled)}
                title={isSoundEnabled ? 'Mute alerts' : 'Unmute alerts'}
              >
                {isSoundEnabled ? '🔊' : '🔇'}
              </button>
              <button
                className="test-sound-btn"
                onClick={() => {
                  playAlertSound('bullish');
                  setTimeout(() => playAlertSound('bearish'), 400);
                }}
                title="Test audio (plays bullish then bearish)"
              >
                🔔
              </button>
              <button
                className="test-cross-btn"
                onClick={() => {
                  // Manually trigger a zero-cross event for testing
                  const direction = currentCVD >= 0 ? 'bearish' : 'bullish';
                  console.log(`🧪 TEST ZERO-CROSS: ${direction.toUpperCase()}`);

                  // Add marker
                  setZeroCrosses(prev => [...prev, {
                    timestamp: Date.now(),
                    direction,
                    x: 0.92,
                    price: lastPrice || 0
                  }]);

                  // Trigger flash
                  setCvdFlashAlert(direction);
                  setTimeout(() => setCvdFlashAlert(null), 500);

                  // Show badge
                  setShowCvdBadge(direction);
                  setTimeout(() => setShowCvdBadge(null), 3000);

                  // Play sound
                  if (isSoundEnabled) {
                    playAlertSound(direction);
                  }
                }}
                title="Test zero-cross alerts (all indicators)"
              >
                ⚡
              </button>
            </>
          )}

          <div className="filter-control">
            <label>MIN SIZE</label>
            <input
              type="number"
              min="1"
              value={minBubbleSize}
              onChange={(e) => setMinBubbleSize(Math.max(1, parseInt(e.target.value) || 1))}
            />
          </div>
          <div className={`status ${isConnected ? (isDemoMode ? 'demo' : 'connected') : ''}`}>
            <span className="status-dot"></span>
            {isConnected ? (isDemoMode ? 'DEMO' : 'LIVE') : 'OFFLINE'}
          </div>
          {isPaused && (
            <div className="paused-indicator" title="Press Space to resume">
              ⏸ PAUSED
            </div>
          )}
          <button
            className="shortcuts-help-btn"
            onClick={() => setShowShortcutsHelp(!showShortcutsHelp)}
            title="Keyboard shortcuts"
          >
            ⌨️
          </button>
        </div>
      </header>

      {/* Keyboard Shortcuts Help Modal */}
      {showShortcutsHelp && (
        <div className="shortcuts-modal-overlay" onClick={() => setShowShortcutsHelp(false)}>
          <div className="shortcuts-modal" onClick={(e) => e.stopPropagation()}>
            <div className="shortcuts-modal-header">
              <h3>⌨️ Keyboard Shortcuts</h3>
              <button className="close-modal-btn" onClick={() => setShowShortcutsHelp(false)}>
                ✕
              </button>
            </div>
            <div className="shortcuts-grid">
              <div className="shortcut-section">
                <h4>General Controls</h4>
                <div className="shortcut-item">
                  <kbd>Space</kbd>
                  <span>Pause/Resume animation</span>
                </div>
                <div className="shortcut-item">
                  <kbd>R</kbd>
                  <span>Reset CVD to zero</span>
                </div>
                <div className="shortcut-item">
                  <kbd>C</kbd>
                  <span>Clear all bubbles</span>
                </div>
                <div className="shortcut-item">
                  <kbd>M</kbd>
                  <span>Mute/Unmute alerts</span>
                </div>
                <div className="shortcut-item">
                  <kbd>S</kbd>
                  <span>Export screenshot</span>
                </div>
              </div>
              <div className="shortcut-section">
                <h4>Filters</h4>
                <div className="shortcut-item">
                  <kbd>1</kbd>
                  <span>Min size: 10 contracts</span>
                </div>
                <div className="shortcut-item">
                  <kbd>2</kbd>
                  <span>Min size: 20 contracts</span>
                </div>
                <div className="shortcut-item">
                  <kbd>3</kbd>
                  <span>Min size: 30 contracts</span>
                </div>
                <div className="shortcut-item">
                  <kbd>4-9</kbd>
                  <span>Min size: 40-90 contracts</span>
                </div>
              </div>
              <div className="shortcut-section">
                <h4>Interactions</h4>
                <div className="shortcut-item">
                  <kbd>Click</kbd>
                  <span>Show bubble details</span>
                </div>
                <div className="shortcut-item">
                  <kbd>Esc</kbd>
                  <span>Close this help</span>
                </div>
              </div>
            </div>
            <div className="shortcuts-modal-footer">
              Press <kbd>?</kbd> or click <span style="font-size: 16px">⌨️</span> to toggle this help
            </div>
          </div>
        </div>
      )}

      {!isConnected ? (
        <div className="connect-panel">
          <div className="connect-card">
            <h2>Connect to Tradovate</h2>
            <p className="connect-subtitle">Enter your API credentials to stream live futures data</p>
            
            {error && <div className="error-message">{error}</div>}
            
            <div className="credentials-grid">
              <div className="input-group">
                <label>Username</label>
                <input
                  type="text"
                  value={credentials.username}
                  onChange={(e) => setCredentials(c => ({ ...c, username: e.target.value }))}
                  placeholder="Tradovate username"
                />
              </div>
              <div className="input-group">
                <label>Password</label>
                <input
                  type="password"
                  value={credentials.password}
                  onChange={(e) => setCredentials(c => ({ ...c, password: e.target.value }))}
                  placeholder="Tradovate password"
                />
              </div>
              <div className="input-group">
                <label>App ID</label>
                <input
                  type="text"
                  value={credentials.appId}
                  onChange={(e) => setCredentials(c => ({ ...c, appId: e.target.value }))}
                  placeholder="Your registered app name"
                />
              </div>
              <div className="input-group">
                <label>CID (Client ID)</label>
                <input
                  type="text"
                  value={credentials.cid}
                  onChange={(e) => setCredentials(c => ({ ...c, cid: e.target.value }))}
                  placeholder="OAuth Client ID"
                />
              </div>
              <div className="input-group full-width">
                <label>Secret</label>
                <input
                  type="password"
                  value={credentials.sec}
                  onChange={(e) => setCredentials(c => ({ ...c, sec: e.target.value }))}
                  placeholder="OAuth Client Secret"
                />
              </div>
            </div>

            <button 
              className="connect-btn"
              onClick={connect}
              disabled={isConnecting || !credentials.username || !credentials.password}
            >
              {isConnecting ? (
                <>
                  <span className="spinner"></span>
                  CONNECTING...
                </>
              ) : (
                'CONNECT'
              )}
            </button>

            <div className="divider">
              <span>or</span>
            </div>

            <button 
              className="demo-btn"
              onClick={startDemo}
            >
              🎮 TRY DEMO MODE
            </button>
            
            <div className="help-text">
              <p>
                Need API credentials? 
                <a href="https://trader.tradovate.com/apikeys" target="_blank" rel="noopener noreferrer">
                  Get them here
                </a>
              </p>
            </div>
          </div>
        </div>
      ) : (
        <div className="visualization">
          {/* Flash Alert Overlay */}
          {cvdFlashAlert && (
            <div className={`flash-alert ${cvdFlashAlert}`}></div>
          )}

          {/* CVD Flip Badge */}
          {showCvdBadge && (
            <div className={`cvd-badge ${showCvdBadge}`}>
              <div className="badge-icon">
                {showCvdBadge === 'bullish' ? '🟢' : '🔴'}
              </div>
              <div className="badge-text">
                CVD FLIP: {showCvdBadge.toUpperCase()}
              </div>
              <div className="badge-subtitle">
                {showCvdBadge === 'bullish' ? 'Buy Signal' : 'Sell Signal'}
              </div>
            </div>
          )}

          <BubbleRenderer
            bubbles={bubbles}
            priceRange={priceRange}
            canvasRef={canvasRef}
            cvdHistory={cvdHistory}
            cvdRange={cvdRange}
            currentCVD={currentCVD}
            zeroCrosses={zeroCrosses}
            onClick={handleCanvasClick}
          />

          {/* Bubble Info Tooltip */}
          {selectedBubble && clickPosition && (
            <div
              className="bubble-info-tooltip"
              style={{
                left: `${clickPosition.x}px`,
                top: `${clickPosition.y}px`
              }}
              onClick={() => {
                setSelectedBubble(null);
                setClickPosition(null);
              }}
            >
              <div className="tooltip-header">
                <span className={`tooltip-side ${selectedBubble.side}`}>
                  {selectedBubble.side.toUpperCase()}
                </span>
                {selectedBubble.isStackedImbalance && (
                  <span className="tooltip-badge">⚡ STACKED</span>
                )}
              </div>
              <div className="tooltip-row">
                <span className="tooltip-label">Size:</span>
                <span className="tooltip-value">{selectedBubble.size} contracts</span>
              </div>
              <div className="tooltip-row">
                <span className="tooltip-label">Price:</span>
                <span className="tooltip-value">{selectedBubble.price.toFixed(2)}</span>
              </div>
              <div className="tooltip-row">
                <span className="tooltip-label">Time:</span>
                <span className="tooltip-value">
                  {new Date(selectedBubble.timestamp).toLocaleTimeString()}
                </span>
              </div>
              <div className="tooltip-footer">Click to close</div>
            </div>
          )}
          <div className="visualization-controls">
            <button className="screenshot-btn" onClick={exportScreenshot} title="Export screenshot (S)">
              📸
            </button>
            <button className="disconnect-btn" onClick={disconnect}>
              DISCONNECT
            </button>
          </div>
        </div>
      )}
      
      <footer className="footer">
        <div className="legend">
          <span className="legend-item buy">
            <span className="legend-dot"></span>
            BUY AGGRESSION
          </span>
          <span className="legend-item sell">
            <span className="legend-dot"></span>
            SELL AGGRESSION
          </span>
        </div>
        {isConnected && (
          <div className="volume-totals">
            <span className="volume-item buy">
              BUY: {totalBuyVolume.toLocaleString()}
            </span>
            <span className="volume-divider">|</span>
            <span className="volume-item sell">
              SELL: {totalSellVolume.toLocaleString()}
            </span>
            <span className="volume-divider">|</span>
            <span className="volume-item delta">
              Δ: {(totalBuyVolume - totalSellVolume > 0 ? '+' : '')}{(totalBuyVolume - totalSellVolume).toLocaleString()}
            </span>
          </div>
        )}
        <div className="bubble-count">
          {bubbles.length} bubbles
        </div>
      </footer>
    </div>
  );
}

export default App;
