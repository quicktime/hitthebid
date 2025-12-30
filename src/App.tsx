import { useEffect, useRef, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { BubbleRenderer } from './BubbleRenderer';
import { StatsPage } from './StatsPage';
import { ReplayControls } from './ReplayControls';
import { SettingsPanel } from './SettingsPanel';
import { DirectionChart } from './DirectionChart';
import { PnLSimulator, SimulatedTrade } from './PnLSimulator';
import { useFlowStore, Bubble, AbsorptionAlert, StackedImbalance, ConfluenceEvent, TradingSignal } from './stores/flowStore';
import { usePreferencesStore } from './stores/preferencesStore';
import './App.css';

const BUBBLE_LIFETIME_SECONDS = 120;

// Audio alert function for zero crosses
function playAlertSound(direction: 'bullish' | 'bearish') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

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

// Audio alert for stacked imbalances - triple ascending/descending beep
function playStackedImbalanceSound(side: 'buy' | 'sell') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const baseFreq = side === 'buy' ? 500 : 400;
    const freqStep = side === 'buy' ? 100 : -50;

    for (let i = 0; i < 3; i++) {
      const osc = audioContext.createOscillator();
      const gain = audioContext.createGain();
      osc.connect(gain);
      gain.connect(audioContext.destination);
      osc.frequency.value = baseFreq + (freqStep * i);
      osc.type = 'square';
      const startTime = audioContext.currentTime + (i * 0.1);
      gain.gain.setValueAtTime(0.15, startTime);
      gain.gain.exponentialRampToValueAtTime(0.01, startTime + 0.08);
      osc.start(startTime);
      osc.stop(startTime + 0.08);
    }
  } catch (e) {
    console.log('Audio not supported', e);
  }
}

// Audio alert for absorption events - double beep
function playAbsorptionSound(type: 'buying' | 'selling') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();

    // First beep
    const osc1 = audioContext.createOscillator();
    const gain1 = audioContext.createGain();
    osc1.connect(gain1);
    gain1.connect(audioContext.destination);
    osc1.frequency.value = type === 'buying' ? 600 : 300;
    osc1.type = 'triangle';
    gain1.gain.setValueAtTime(0.2, audioContext.currentTime);
    gain1.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.1);
    osc1.start(audioContext.currentTime);
    osc1.stop(audioContext.currentTime + 0.1);

    // Second beep (slightly delayed)
    const osc2 = audioContext.createOscillator();
    const gain2 = audioContext.createGain();
    osc2.connect(gain2);
    gain2.connect(audioContext.destination);
    osc2.frequency.value = type === 'buying' ? 700 : 350;
    osc2.type = 'triangle';
    gain2.gain.setValueAtTime(0.2, audioContext.currentTime + 0.15);
    gain2.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.25);
    osc2.start(audioContext.currentTime + 0.15);
    osc2.stop(audioContext.currentTime + 0.25);
  } catch (e) {
    console.log('Audio not supported', e);
  }
}

// Audio alert for confluence - distinctive chord
function playConfluenceSound(direction: 'bullish' | 'bearish') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
    const baseFreq = direction === 'bullish' ? 400 : 300;
    const freqs = [baseFreq, baseFreq * 1.25, baseFreq * 1.5]; // Major chord

    freqs.forEach((freq, i) => {
      const osc = audioContext.createOscillator();
      const gain = audioContext.createGain();
      osc.connect(gain);
      gain.connect(audioContext.destination);
      osc.frequency.value = freq;
      osc.type = 'sine';
      const delay = i * 0.05;
      gain.gain.setValueAtTime(0.15, audioContext.currentTime + delay);
      gain.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + delay + 0.4);
      osc.start(audioContext.currentTime + delay);
      osc.stop(audioContext.currentTime + delay + 0.4);
    });
  } catch (e) {
    console.log('Audio not supported', e);
  }
}

// Audio alert for trading signals - urgent fanfare for entry, resolution for exit
function playTradingSignalSound(signalType: 'entry' | 'exit', direction: 'long' | 'short') {
  try {
    const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();

    if (signalType === 'entry') {
      // Entry: Ascending fanfare
      const baseFreq = direction === 'long' ? 440 : 330;
      const freqs = [baseFreq, baseFreq * 1.25, baseFreq * 1.5, baseFreq * 2];

      freqs.forEach((freq, i) => {
        const osc = audioContext.createOscillator();
        const gain = audioContext.createGain();
        osc.connect(gain);
        gain.connect(audioContext.destination);
        osc.frequency.value = freq;
        osc.type = 'sine';
        const startTime = audioContext.currentTime + (i * 0.1);
        gain.gain.setValueAtTime(0.25, startTime);
        gain.gain.exponentialRampToValueAtTime(0.01, startTime + 0.15);
        osc.start(startTime);
        osc.stop(startTime + 0.15);
      });

      // Final sustained note
      const finalOsc = audioContext.createOscillator();
      const finalGain = audioContext.createGain();
      finalOsc.connect(finalGain);
      finalGain.connect(audioContext.destination);
      finalOsc.frequency.value = baseFreq * 2;
      finalOsc.type = 'triangle';
      finalGain.gain.setValueAtTime(0.3, audioContext.currentTime + 0.4);
      finalGain.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.8);
      finalOsc.start(audioContext.currentTime + 0.4);
      finalOsc.stop(audioContext.currentTime + 0.8);
    } else {
      // Exit: Resolution tone
      const baseFreq = direction === 'long' ? 523 : 392;
      const osc = audioContext.createOscillator();
      const gain = audioContext.createGain();
      osc.connect(gain);
      gain.connect(audioContext.destination);
      osc.frequency.value = baseFreq;
      osc.type = 'sine';
      gain.gain.setValueAtTime(0.2, audioContext.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.5);
      osc.start(audioContext.currentTime);
      osc.stop(audioContext.currentTime + 0.5);
    }
  } catch (e) {
    console.log('Audio not supported', e);
  }
}

function App() {
  // Zustand stores
  const {
    isConnected,
    error,
    serverMode,
    connectedSymbols: _connectedSymbols,
    lastPrice,
    priceRange,
    currentCVD,
    cvdHistory,
    cvdRange,
    zeroCrosses,
    cvdStartTime,
    bubbles,
    volumeProfile,
    absorptionZones,
    stackedImbalances,
    tradingSignals,
    activeTradingSignal,
    sessionStats,
    replayStatus,
    isPaused,
    connect,
    disconnect,
    resetCVD,
    pause,
    resume,
    togglePause,
    setReplaySpeed,
    setMinSize: setMinSizeWs,
    clearBubbles,
    animateFrame,
    cleanupOldItems,
    clearError,
    getFilteredBubbles: _getFilteredBubbles,
  } = useFlowStore();

  const {
    isSoundEnabled,
    minSize,
    selectedSymbol,
    notificationsEnabled,
    setMinSize,
    setSymbol: _setSelectedSymbol,
    setNotifications: setNotificationsEnabled,
    toggleSound,
  } = usePreferencesStore();

  // UI-only local state (not shared between components)
  const [cvdFlashAlert, setCvdFlashAlert] = useState<'bullish' | 'bearish' | null>(null);
  const [showCvdBadge, setShowCvdBadge] = useState<'bullish' | 'bearish' | null>(null);
  const [selectedBubble, setSelectedBubble] = useState<Bubble | null>(null);
  const [clickPosition, setClickPosition] = useState<{ x: number; y: number } | null>(null);
  const [showShortcutsHelp, setShowShortcutsHelp] = useState(false);
  const [showAbsorptionBadge, setShowAbsorptionBadge] = useState<AbsorptionAlert | null>(null);
  const [showStackedBadge, setShowStackedBadge] = useState<StackedImbalance | null>(null);
  const [showConfluenceBadge, setShowConfluenceBadge] = useState<ConfluenceEvent | null>(null);
  const [showTradingSignalBadge, setShowTradingSignalBadge] = useState<TradingSignal | null>(null);
  const [currentView, setCurrentView] = useState<'chart' | 'stats' | 'history'>('chart');
  const [showSettings, setShowSettings] = useState(false);
  const [notificationsPermission, setNotificationsPermission] = useState<NotificationPermission>('default');
  const [showPnLSimulator, setShowPnLSimulator] = useState(false);
  const [simulatedTrades, setSimulatedTrades] = useState<SimulatedTrade[]>([]);

  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Use all bubbles directly (NQ only now, no filtering needed)
  const filteredBubbles = bubbles;

  // Track previous signal counts to detect new signals
  const prevZeroCrossCount = useRef(0);
  const prevStackedCount = useRef(0);
  const prevConfluenceCount = useRef(0);
  const prevTradingSignalCount = useRef(0);
  const { absorptionAlerts } = useFlowStore();
  const prevAbsorptionCount = useRef(0);

  // Connect to Rust backend via store
  useEffect(() => {
    connect();
    return () => disconnect();
  }, [connect, disconnect]);

  // React to new zero crosses (CVD flips) with UI effects
  useEffect(() => {
    if (zeroCrosses.length > prevZeroCrossCount.current) {
      const latest = zeroCrosses[zeroCrosses.length - 1];

      // Flash alert
      setCvdFlashAlert(latest.direction);
      setTimeout(() => setCvdFlashAlert(null), 500);

      // Badge
      setShowCvdBadge(latest.direction);
      setTimeout(() => setShowCvdBadge(null), 3000);

      // Sound
      if (isSoundEnabled) {
        playAlertSound(latest.direction);
      }
    }
    prevZeroCrossCount.current = zeroCrosses.length;
  }, [zeroCrosses, isSoundEnabled]);

  // React to new absorption alerts with UI effects
  useEffect(() => {
    if (absorptionAlerts.length > prevAbsorptionCount.current) {
      const latest = absorptionAlerts[absorptionAlerts.length - 1];

      // Only show badge for medium+ strength
      if (latest.strength !== 'weak') {
        setShowAbsorptionBadge(latest);
        setTimeout(() => setShowAbsorptionBadge(null), 4000);

        if (isSoundEnabled) {
          playAbsorptionSound(latest.absorptionType);
        }
      }
    }
    prevAbsorptionCount.current = absorptionAlerts.length;
  }, [absorptionAlerts, isSoundEnabled]);

  // React to new stacked imbalances with UI effects
  useEffect(() => {
    if (stackedImbalances.length > prevStackedCount.current) {
      const latest = stackedImbalances[stackedImbalances.length - 1];

      // Show badge for 4+ levels (strong signal)
      if (latest.levelCount >= 4) {
        setShowStackedBadge(latest);
        setTimeout(() => setShowStackedBadge(null), 3000);

        if (isSoundEnabled) {
          playStackedImbalanceSound(latest.side);
        }

        // Browser notification
        if (Notification.permission === 'granted' && !document.hasFocus()) {
          new Notification(`Stacked Imbalance ${latest.side.toUpperCase()}`, {
            body: `${latest.levelCount} levels from ${latest.priceLow.toFixed(2)} to ${latest.priceHigh.toFixed(2)}`,
            tag: `stacked-${latest.timestamp}`,
            icon: '/favicon.ico',
          });
        }
      }
    }
    prevStackedCount.current = stackedImbalances.length;
  }, [stackedImbalances, isSoundEnabled]);

  // React to new confluence events with UI effects
  const { confluenceEvents } = useFlowStore();
  useEffect(() => {
    if (confluenceEvents.length > prevConfluenceCount.current) {
      const latest = confluenceEvents[confluenceEvents.length - 1];

      // Always show confluence badge
      setShowConfluenceBadge(latest);
      setTimeout(() => setShowConfluenceBadge(null), 5000);

      if (isSoundEnabled) {
        playConfluenceSound(latest.direction);
      }

      // Browser notification
      if (Notification.permission === 'granted' && !document.hasFocus()) {
        new Notification(`Confluence ${latest.direction.toUpperCase()}`, {
          body: `${latest.signals.join(' + ')} at ${latest.price.toFixed(2)}`,
          tag: `confluence-${latest.timestamp}`,
          icon: '/favicon.ico',
        });
      }
    }
    prevConfluenceCount.current = confluenceEvents.length;
  }, [confluenceEvents, isSoundEnabled]);

  // React to new trading signals with UI effects
  useEffect(() => {
    if (tradingSignals.length > prevTradingSignalCount.current) {
      const latest = tradingSignals[tradingSignals.length - 1];

      // Only show badge for entry and exit signals
      if (latest.signalType === 'entry' || latest.signalType === 'exit') {
        setShowTradingSignalBadge(latest);

        // Keep entry signals visible longer
        const timeout = latest.signalType === 'entry' ? 10000 : 5000;
        setTimeout(() => setShowTradingSignalBadge(null), timeout);

        // Sound
        if (isSoundEnabled && latest.direction) {
          playTradingSignalSound(
            latest.signalType as 'entry' | 'exit',
            latest.direction as 'long' | 'short'
          );
        }

        // Browser notification
        if (Notification.permission === 'granted' && !document.hasFocus()) {
          const title = latest.signalType === 'entry'
            ? `TRADE SIGNAL: ${latest.direction.toUpperCase()}`
            : `TRADE EXIT: ${latest.reason || 'Closed'}`;
          const body = latest.signalType === 'entry'
            ? `Entry: ${latest.price.toFixed(2)} | Stop: ${latest.stop?.toFixed(2)} | Target: ${latest.target?.toFixed(2)}`
            : `P&L: ${latest.pnlPoints?.toFixed(2)} pts`;

          new Notification(title, {
            body,
            tag: `trading-${latest.timestamp}`,
            icon: '/favicon.ico',
            requireInteraction: latest.signalType === 'entry',
          });
        }
      }
    }
    prevTradingSignalCount.current = tradingSignals.length;
  }, [tradingSignals, isSoundEnabled]);

  // Replay control callbacks using store actions
  const handleReplayPause = useCallback(() => {
    pause();
  }, [pause]);

  const handleReplayResume = useCallback(() => {
    resume();
  }, [resume]);

  const handleReplaySpeedChange = useCallback((speed: number) => {
    setReplaySpeed(speed);
  }, [setReplaySpeed]);

  const handleMinSizeChange = useCallback((size: number) => {
    setMinSizeWs(size);
    setMinSize(size);
  }, [setMinSizeWs, setMinSize]);

  // Request notification permission
  const requestNotificationPermission = useCallback(async () => {
    if (!('Notification' in window)) {
      console.log('Browser does not support notifications');
      return;
    }
    const permission = await Notification.requestPermission();
    setNotificationsPermission(permission);
    if (permission === 'granted') {
      setNotificationsEnabled(true);
    }
  }, []);

  // P&L Simulator handlers
  const handleAddTrade = useCallback((trade: Omit<SimulatedTrade, 'id' | 'status'>) => {
    const newTrade: SimulatedTrade = {
      ...trade,
      id: `trade-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      status: 'open',
    };
    setSimulatedTrades((prev) => [...prev, newTrade]);
    console.log(`[PnL] Opened ${trade.direction.toUpperCase()} @ ${trade.entryPrice.toFixed(2)}`);
  }, []);

  const handleCloseTrade = useCallback((tradeId: string, exitPrice: number) => {
    setSimulatedTrades((prev) =>
      prev.map((trade) => {
        if (trade.id !== tradeId || trade.status === 'closed') return trade;
        const priceDiff = exitPrice - trade.entryPrice;
        const pnl = trade.direction === 'long' ? priceDiff * trade.size : -priceDiff * trade.size;
        console.log(`[PnL] Closed ${trade.direction.toUpperCase()} @ ${exitPrice.toFixed(2)} | P&L: ${pnl >= 0 ? '+' : ''}${pnl.toFixed(2)}`);
        return {
          ...trade,
          exitTime: Date.now(),
          exitPrice,
          pnl,
          status: 'closed' as const,
        };
      })
    );
  }, []);

  const handleClearTrades = useCallback(() => {
    setSimulatedTrades([]);
    console.log('[PnL] Cleared all trades');
  }, []);

  // Quick trade from signal badge
  const enterTradeFromSignal = useCallback(
    (direction: 'bullish' | 'bearish', signalType: string, price?: number) => {
      if (!lastPrice && !price) return;
      handleAddTrade({
        entryTime: Date.now(),
        entryPrice: price || lastPrice!,
        direction: direction === 'bullish' ? 'long' : 'short',
        size: 1,
        signalType,
      });
    },
    [lastPrice, handleAddTrade]
  );

  // Check notification permission on mount
  useEffect(() => {
    if ('Notification' in window) {
      setNotificationsPermission(Notification.permission);
      setNotificationsEnabled(Notification.permission === 'granted');
    }
  }, []);

  // Export screenshot
  const exportScreenshot = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    try {
      canvas.toBlob((blob) => {
        if (!blob) return;

        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
        link.download = `flow-orderflow-${timestamp}.png`;
        link.href = url;
        link.click();

        URL.revokeObjectURL(url);
        console.log('📸 Screenshot exported');
      }, 'image/png');
    } catch (err) {
      console.error('Failed to export screenshot:', err);
    }
  }, []);

  // Handle canvas click to show bubble info
  const handleCanvasClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      if (!canvas || !priceRange) return;

      const rect = canvas.getBoundingClientRect();
      const clickX = e.clientX - rect.left;
      const clickY = e.clientY - rect.top;

      const normalizedX = clickX / rect.width;
      const normalizedY = clickY / rect.height;

      const priceSpan = priceRange.max - priceRange.min;

      let clickedBubble: Bubble | null = null;
      let minDistance = Infinity;

      for (let i = filteredBubbles.length - 1; i >= 0; i--) {
        const bubble = filteredBubbles[i];
        const bubbleX = bubble.x;
        const bubbleY = 1 - (bubble.price - priceRange.min) / priceSpan;
        const radius = Math.min(100, Math.max(3, bubble.size * 0.008)) / rect.width;

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
    },
    [filteredBubbles, priceRange]
  );

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyPress = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).tagName === 'INPUT') return;

      const key = e.key.toLowerCase();

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
        setShowShortcutsHelp((prev) => !prev);
        return;
      }

      if (showShortcutsHelp) return;

      switch (key) {
        case 'r':
          resetCVD();
          console.log('Keyboard: CVD Reset (R)');
          break;
        case ' ':
          e.preventDefault();
          togglePause();
          console.log(`Keyboard: ${isPaused ? 'Resumed' : 'Paused'} (Space)`);
          break;
        case 'c':
          clearBubbles();
          console.log('Keyboard: Cleared bubbles (C)');
          break;
        case 'm':
          toggleSound();
          console.log(`Keyboard: Sound ${isSoundEnabled ? 'Muted' : 'Enabled'} (M)`);
          break;
        case 's':
          exportScreenshot();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyPress);
    return () => window.removeEventListener('keydown', handleKeyPress);
  }, [resetCVD, exportScreenshot, showShortcutsHelp, selectedBubble, togglePause, isPaused, clearBubbles, toggleSound, isSoundEnabled]);

  // Animation loop - TIME-BASED
  useEffect(() => {
    let animationFrameId: number;
    let lastFrameTime = performance.now();
    let lastCleanupTime = performance.now();

    const SPEED_PER_SECOND = (0.77 / BUBBLE_LIFETIME_SECONDS) * 3; // 3x faster panning
    const CLEANUP_INTERVAL_MS = 1000;
    const MAX_AGE_MS = BUBBLE_LIFETIME_SECONDS * 1000;

    const animate = (currentTime: number) => {
      if (isPaused) {
        lastFrameTime = currentTime;
        lastCleanupTime = currentTime;
        animationFrameId = requestAnimationFrame(animate);
        return;
      }

      const deltaTime = (currentTime - lastFrameTime) / 1000;
      lastFrameTime = currentTime;

      const movement = SPEED_PER_SECOND * deltaTime;

      // Cleanup old items periodically
      const shouldCleanup = currentTime - lastCleanupTime >= CLEANUP_INTERVAL_MS;
      if (shouldCleanup) {
        lastCleanupTime = currentTime;
        cleanupOldItems(MAX_AGE_MS);
      }

      // Animate all items
      animateFrame(movement);

      animationFrameId = requestAnimationFrame(animate);
    };

    animationFrameId = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(animationFrameId);
  }, [isPaused, animateFrame, cleanupOldItems]);

  return (
    <div className="app">
      <header className="header">
        <div className="header-left">
          <h1 className="logo">
            <span className="logo-icon">◉</span>
            HIT
          </h1>
          <div className="symbol-selector">
            <Link to="/flow" className="symbol-btn active">
              Flow
            </Link>
            <Link to="/journal" className="symbol-btn">
              Journal
            </Link>
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
          {isConnected && (
            <>
              <div className={`cvd-widget ${currentCVD >= 0 ? 'bullish' : 'bearish'}`}>
                <label>CVD</label>
                <div className="cvd-value">
                  {currentCVD > 0 ? '+' : ''}
                  {currentCVD.toFixed(0)}
                </div>
                <div className="cvd-direction">
                  {currentCVD >= 0 ? '↗ BULLISH' : '↘ BEARISH'}
                </div>
                <div className="cvd-age">
                  Since{' '}
                  {new Date(cvdStartTime).toLocaleTimeString('en-US', {
                    hour: 'numeric',
                    minute: '2-digit',
                    hour12: true,
                  })}
                </div>
              </div>
              <button className="reset-cvd-btn" onClick={resetCVD} title="Reset CVD to zero">
                🔄
              </button>
              <button
                className={`sound-toggle-btn ${isSoundEnabled ? 'enabled' : 'disabled'}`}
                onClick={toggleSound}
                title={isSoundEnabled ? 'Mute alerts' : 'Unmute alerts'}
              >
                {isSoundEnabled ? '🔊' : '🔇'}
              </button>
            </>
          )}

          <div className={`status ${isConnected ? (serverMode === 'demo' ? 'demo' : serverMode === 'replay' ? 'replay' : 'connected') : ''}`}>
            <span className="status-dot"></span>
            {isConnected ? serverMode.toUpperCase() : 'OFFLINE'}
          </div>
          {isConnected && serverMode === 'replay' && replayStatus && (
            <ReplayControls
              status={replayStatus}
              onPause={handleReplayPause}
              onResume={handleReplayResume}
              onSpeedChange={handleReplaySpeedChange}
            />
          )}
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
          {isConnected && (
            <button
              className="settings-btn"
              onClick={() => setShowSettings(true)}
              title="Settings"
            >
              ⚙️
            </button>
          )}
          {isConnected && (
            <button className="screenshot-btn" onClick={exportScreenshot} title="Export screenshot (S)">
              📸
            </button>
          )}
          {isConnected && (
            <button
              className={`pnl-btn ${simulatedTrades.filter(t => t.status === 'open').length > 0 ? 'has-positions' : ''}`}
              onClick={() => setShowPnLSimulator(true)}
              title="P&L Simulator"
            >
              💰 {simulatedTrades.filter(t => t.status === 'open').length > 0 && (
                <span className="position-count">{simulatedTrades.filter(t => t.status === 'open').length}</span>
              )}
            </button>
          )}
          {isConnected && (
            <button
              className={`view-toggle-btn ${currentView === 'stats' ? 'active' : ''}`}
              onClick={() => setCurrentView(currentView === 'stats' ? 'chart' : 'stats')}
              title={currentView === 'stats' ? 'Back to Chart' : 'View Session Stats'}
            >
              📊
            </button>
          )}
          <button
            className={`view-toggle-btn ${currentView === 'history' ? 'active' : ''}`}
            onClick={() => setCurrentView(currentView === 'history' ? 'chart' : 'history')}
            title={currentView === 'history' ? 'Back to Chart' : 'View Historical Stats'}
          >
            📜
          </button>
        </div>
      </header>

      {/* Settings Panel */}
      <SettingsPanel
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        minSize={minSize}
        onMinSizeChange={handleMinSizeChange}
        isSoundEnabled={isSoundEnabled}
        onSoundToggle={toggleSound}
        notificationsEnabled={notificationsEnabled}
        notificationsPermission={notificationsPermission}
        onRequestNotificationPermission={requestNotificationPermission}
      />

      {/* P&L Simulator */}
      <PnLSimulator
        isOpen={showPnLSimulator}
        onClose={() => setShowPnLSimulator(false)}
        currentPrice={lastPrice}
        trades={simulatedTrades}
        onAddTrade={handleAddTrade}
        onCloseTrade={handleCloseTrade}
        onClearTrades={handleClearTrades}
      />

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
              Press <kbd>?</kbd> or click <span style={{ fontSize: '16px' }}>⌨️</span> to toggle
              this help
            </div>
          </div>
        </div>
      )}

      {error && (
        <div className="error-banner">
          ⚠️ {error}
          <button onClick={clearError}>✕</button>
        </div>
      )}

      <div className="visualization">
        {/* Flash Alert Overlay */}
        {cvdFlashAlert && <div className={`flash-alert ${cvdFlashAlert}`}></div>}

        {/* CVD Flip Badge */}
        {showCvdBadge && (
          <div className={`cvd-badge ${showCvdBadge}`}>
            <div className="badge-icon">{showCvdBadge === 'bullish' ? '🟢' : '🔴'}</div>
            <div className="badge-text">CVD FLIP: {showCvdBadge.toUpperCase()}</div>
            <button
              className={`badge-trade-btn ${showCvdBadge}`}
              onClick={() => enterTradeFromSignal(showCvdBadge, 'delta_flip')}
            >
              {showCvdBadge === 'bullish' ? 'LONG' : 'SHORT'} @ {lastPrice?.toFixed(2) || 'Market'}
            </button>
            <div className="badge-subtitle">
              {showCvdBadge === 'bullish' ? 'Buy Signal' : 'Sell Signal'}
            </div>
          </div>
        )}

        {/* Absorption Badge */}
        {showAbsorptionBadge && (
          <div className={`absorption-badge ${showAbsorptionBadge.absorptionType} ${showAbsorptionBadge.strength}`}>
            <div className={`strength-indicator ${showAbsorptionBadge.strength}`}>
              {showAbsorptionBadge.strength.toUpperCase()}
            </div>
            <div className="badge-icon">
              {showAbsorptionBadge.strength === 'defended' ? '🔥' : '🛡️'}
            </div>
            <div className="badge-text">
              {showAbsorptionBadge.strength === 'defended' ? 'DEFENDED LEVEL' : 'ABSORPTION'}
            </div>
            <div className="badge-type">
              {showAbsorptionBadge.absorptionType === 'buying'
                ? 'Sellers absorbing buyers'
                : 'Buyers absorbing sellers'}
            </div>
            <div className="badge-stats">
              <span className="stat">
                <span className="stat-label">Events</span>
                <span className="stat-value">{showAbsorptionBadge.eventCount}x</span>
              </span>
              <span className="stat">
                <span className="stat-label">Volume</span>
                <span className="stat-value">{showAbsorptionBadge.totalAbsorbed}</span>
              </span>
              <span className="stat">
                <span className="stat-label">Price</span>
                <span className="stat-value">{showAbsorptionBadge.price.toFixed(2)}</span>
              </span>
            </div>
            {(showAbsorptionBadge.atKeyLevel || showAbsorptionBadge.againstTrend) && (
              <div className="badge-context">
                {showAbsorptionBadge.atKeyLevel && <span className="context-tag key-level">@ KEY LEVEL</span>}
                {showAbsorptionBadge.againstTrend && <span className="context-tag against-trend">⚠️ AGAINST TREND</span>}
              </div>
            )}
            <div className="badge-subtitle">
              {showAbsorptionBadge.strength === 'defended'
                ? 'High probability reversal zone'
                : showAbsorptionBadge.strength === 'strong'
                ? 'Strong institutional defense'
                : 'Building absorption zone'}
            </div>
          </div>
        )}

        {/* Stacked Imbalance Badge */}
        {showStackedBadge && (
          <div className={`stacked-badge ${showStackedBadge.side}`}>
            <div className="badge-icon">
              {showStackedBadge.side === 'buy' ? '📈' : '📉'}
            </div>
            <div className="badge-text">STACKED IMBALANCE</div>
            <div className="badge-type">
              {showStackedBadge.levelCount} consecutive {showStackedBadge.side} levels
            </div>
            <div className="badge-stats">
              <span className="stat">
                <span className="stat-label">Levels</span>
                <span className="stat-value">{showStackedBadge.levelCount}</span>
              </span>
              <span className="stat">
                <span className="stat-label">Range</span>
                <span className="stat-value">{showStackedBadge.priceLow.toFixed(2)} - {showStackedBadge.priceHigh.toFixed(2)}</span>
              </span>
            </div>
            <button
              className={`badge-trade-btn ${showStackedBadge.side === 'buy' ? 'bullish' : 'bearish'}`}
              onClick={() => enterTradeFromSignal(
                showStackedBadge.side === 'buy' ? 'bullish' : 'bearish',
                'stacked_imbalance'
              )}
            >
              {showStackedBadge.side === 'buy' ? 'LONG' : 'SHORT'} @ {lastPrice?.toFixed(2) || 'Market'}
            </button>
            <div className="badge-subtitle">
              Strong {showStackedBadge.side === 'buy' ? 'buying' : 'selling'} pressure - expect continuation
            </div>
          </div>
        )}

        {/* Confluence Badge */}
        {showConfluenceBadge && (
          <div className={`confluence-badge ${showConfluenceBadge.direction}`}>
            <div className="badge-icon">🎯</div>
            <div className="badge-text">CONFLUENCE</div>
            <div className="badge-type">
              {showConfluenceBadge.score >= 3 ? 'HIGH PROBABILITY' : 'MEDIUM PROBABILITY'} {showConfluenceBadge.direction.toUpperCase()}
            </div>
            <div className="badge-signals">
              {showConfluenceBadge.signals.map((signal, i) => (
                <span key={i} className="signal-tag">{signal.replace('_', ' ')}</span>
              ))}
            </div>
            <div className="badge-stats">
              <span className="stat">
                <span className="stat-label">Score</span>
                <span className="stat-value">{showConfluenceBadge.score}/4</span>
              </span>
              <span className="stat">
                <span className="stat-label">Price</span>
                <span className="stat-value">{showConfluenceBadge.price.toFixed(2)}</span>
              </span>
            </div>
            <button
              className={`badge-trade-btn ${showConfluenceBadge.direction}`}
              onClick={() => enterTradeFromSignal(showConfluenceBadge.direction, 'confluence', showConfluenceBadge.price)}
            >
              {showConfluenceBadge.direction === 'bullish' ? 'LONG' : 'SHORT'} @ {showConfluenceBadge.price.toFixed(2)}
            </button>
            <div className="badge-subtitle">
              {showConfluenceBadge.direction === 'bullish'
                ? 'Multiple signals agree - consider LONG entry'
                : 'Multiple signals agree - consider SHORT entry'}
            </div>
          </div>
        )}

        {/* Trading Signal Entry Badge */}
        {showTradingSignalBadge && showTradingSignalBadge.signalType === 'entry' && (
          <div className={`trading-signal-badge ${showTradingSignalBadge.direction}`}>
            <div className="signal-header">
              <div className="badge-icon">{showTradingSignalBadge.direction === 'long' ? '🚀' : '🔻'}</div>
              <div className="badge-text">
                {showTradingSignalBadge.direction.toUpperCase()} SIGNAL
              </div>
            </div>
            <div className="signal-details">
              <div className="signal-row">
                <span className="signal-label">Entry</span>
                <span className="signal-value entry">{showTradingSignalBadge.price.toFixed(2)}</span>
              </div>
              <div className="signal-row">
                <span className="signal-label">Stop</span>
                <span className="signal-value stop">{showTradingSignalBadge.stop?.toFixed(2)}</span>
              </div>
              <div className="signal-row">
                <span className="signal-label">Target</span>
                <span className="signal-value target">{showTradingSignalBadge.target?.toFixed(2)}</span>
              </div>
            </div>
            <button
              className={`badge-trade-btn ${showTradingSignalBadge.direction === 'long' ? 'bullish' : 'bearish'}`}
              onClick={() => enterTradeFromSignal(
                showTradingSignalBadge.direction === 'long' ? 'bullish' : 'bearish',
                'lvn_retest',
                showTradingSignalBadge.price
              )}
            >
              EXECUTE {showTradingSignalBadge.direction.toUpperCase()}
            </button>
            <div className="badge-subtitle">
              Delta Confirmation Signal
            </div>
          </div>
        )}

        {/* Trading Signal Exit Badge */}
        {showTradingSignalBadge && showTradingSignalBadge.signalType === 'exit' && (
          <div className={`trading-exit-badge ${(showTradingSignalBadge.pnlPoints || 0) >= 0 ? 'win' : 'loss'}`}>
            <div className="badge-icon">{(showTradingSignalBadge.pnlPoints || 0) >= 0 ? '✅' : '❌'}</div>
            <div className="badge-text">TRADE CLOSED</div>
            <div className="exit-reason">{showTradingSignalBadge.reason}</div>
            <div className="exit-pnl">
              <span className="pnl-label">P&L:</span>
              <span className={`pnl-value ${(showTradingSignalBadge.pnlPoints || 0) >= 0 ? 'positive' : 'negative'}`}>
                {(showTradingSignalBadge.pnlPoints || 0) >= 0 ? '+' : ''}{showTradingSignalBadge.pnlPoints?.toFixed(2)} pts
              </span>
            </div>
          </div>
        )}

        {/* Active Position Indicator */}
        {activeTradingSignal && !showTradingSignalBadge && (
          <div className={`active-position-indicator ${activeTradingSignal.direction}`}>
            <span className="position-icon">{activeTradingSignal.direction === 'long' ? '🔼' : '🔽'}</span>
            <span className="position-label">{activeTradingSignal.direction.toUpperCase()}</span>
            <span className="position-entry">@ {activeTradingSignal.price.toFixed(2)}</span>
            <span className="position-stop">Stop: {activeTradingSignal.stop?.toFixed(2)}</span>
          </div>
        )}

        {currentView === 'chart' ? (
          <>
            <BubbleRenderer
              bubbles={filteredBubbles}
              priceRange={priceRange}
              canvasRef={canvasRef}
              cvdHistory={cvdHistory}
              cvdRange={cvdRange}
              currentCVD={currentCVD}
              zeroCrosses={zeroCrosses}
              onClick={handleCanvasClick}
              volumeProfile={volumeProfile}
              absorptionZones={absorptionZones}
              stackedImbalances={stackedImbalances}
            />

            {/* Bubble Info Tooltip */}
            {selectedBubble && clickPosition && (
              <div
                className="bubble-info-tooltip"
                style={{
                  left: `${clickPosition.x}px`,
                  top: `${clickPosition.y}px`,
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
          </>
        ) : currentView === 'history' ? (
          <StatsPage onClose={() => setCurrentView('chart')} />
        ) : (
          /* Session Stats View */
          <div className="stats-view">
            <h2>Session Statistics</h2>
            {sessionStats ? (
              <div className="stats-grid">
                <div className="stats-overview">
                  <div className="overview-card">
                    <span className="overview-label">Session Start</span>
                    <span className="overview-value">
                      {new Date(sessionStats.sessionStart).toLocaleTimeString()}
                    </span>
                  </div>
                  <div className="overview-card">
                    <span className="overview-label">Current Price</span>
                    <span className="overview-value">{sessionStats.currentPrice.toFixed(2)}</span>
                  </div>
                  <div className="overview-card">
                    <span className="overview-label">Session Range</span>
                    <span className="overview-value">
                      {sessionStats.sessionLow.toFixed(2)} - {sessionStats.sessionHigh.toFixed(2)}
                    </span>
                  </div>
                  <div className="overview-card">
                    <span className="overview-label">Total Volume</span>
                    <span className="overview-value">{sessionStats.totalVolume.toLocaleString()}</span>
                  </div>
                </div>

                <div className="direction-charts-grid">
                  <DirectionChart
                    title="Delta Flips"
                    bullish={sessionStats.deltaFlips.bullishCount}
                    bearish={sessionStats.deltaFlips.bearishCount}
                  />
                  <DirectionChart
                    title="Absorptions"
                    bullish={sessionStats.absorptions.bullishCount}
                    bearish={sessionStats.absorptions.bearishCount}
                  />
                  <DirectionChart
                    title="Stacked Imbalances"
                    bullish={sessionStats.stackedImbalances.bullishCount}
                    bearish={sessionStats.stackedImbalances.bearishCount}
                  />
                  <DirectionChart
                    title="Confluences"
                    bullish={sessionStats.confluences.bullishCount}
                    bearish={sessionStats.confluences.bearishCount}
                  />
                </div>

                <div className="signal-stats-grid">
                  {/* Delta Flips */}
                  <div className="signal-card">
                    <h3>Delta Flips</h3>
                    <div className="signal-counts">
                      <span className="count total">{sessionStats.deltaFlips.count} total</span>
                      <span className="count bullish">{sessionStats.deltaFlips.bullishCount} bullish</span>
                      <span className="count bearish">{sessionStats.deltaFlips.bearishCount} bearish</span>
                    </div>
                    <div className="signal-metrics">
                      <div className="metric">
                        <span className="metric-label">Win Rate</span>
                        <span className={`metric-value ${sessionStats.deltaFlips.winRate >= 50 ? 'positive' : 'negative'}`}>
                          {sessionStats.deltaFlips.winRate.toFixed(1)}%
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (1m)</span>
                        <span className={`metric-value ${sessionStats.deltaFlips.avgMove1m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.deltaFlips.avgMove1m >= 0 ? '+' : ''}{sessionStats.deltaFlips.avgMove1m.toFixed(2)}
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (5m)</span>
                        <span className={`metric-value ${sessionStats.deltaFlips.avgMove5m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.deltaFlips.avgMove5m >= 0 ? '+' : ''}{sessionStats.deltaFlips.avgMove5m.toFixed(2)}
                        </span>
                      </div>
                    </div>
                    <div className="win-loss">
                      <span className="wins">{sessionStats.deltaFlips.wins} W</span>
                      <span className="losses">{sessionStats.deltaFlips.losses} L</span>
                    </div>
                  </div>

                  {/* Absorptions */}
                  <div className="signal-card">
                    <h3>Absorptions</h3>
                    <div className="signal-counts">
                      <span className="count total">{sessionStats.absorptions.count} total</span>
                      <span className="count bullish">{sessionStats.absorptions.bullishCount} bullish</span>
                      <span className="count bearish">{sessionStats.absorptions.bearishCount} bearish</span>
                    </div>
                    <div className="signal-metrics">
                      <div className="metric">
                        <span className="metric-label">Win Rate</span>
                        <span className={`metric-value ${sessionStats.absorptions.winRate >= 50 ? 'positive' : 'negative'}`}>
                          {sessionStats.absorptions.winRate.toFixed(1)}%
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (1m)</span>
                        <span className={`metric-value ${sessionStats.absorptions.avgMove1m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.absorptions.avgMove1m >= 0 ? '+' : ''}{sessionStats.absorptions.avgMove1m.toFixed(2)}
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (5m)</span>
                        <span className={`metric-value ${sessionStats.absorptions.avgMove5m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.absorptions.avgMove5m >= 0 ? '+' : ''}{sessionStats.absorptions.avgMove5m.toFixed(2)}
                        </span>
                      </div>
                    </div>
                    <div className="win-loss">
                      <span className="wins">{sessionStats.absorptions.wins} W</span>
                      <span className="losses">{sessionStats.absorptions.losses} L</span>
                    </div>
                  </div>

                  {/* Stacked Imbalances */}
                  <div className="signal-card">
                    <h3>Stacked Imbalances</h3>
                    <div className="signal-counts">
                      <span className="count total">{sessionStats.stackedImbalances.count} total</span>
                      <span className="count bullish">{sessionStats.stackedImbalances.bullishCount} bullish</span>
                      <span className="count bearish">{sessionStats.stackedImbalances.bearishCount} bearish</span>
                    </div>
                    <div className="signal-metrics">
                      <div className="metric">
                        <span className="metric-label">Win Rate</span>
                        <span className={`metric-value ${sessionStats.stackedImbalances.winRate >= 50 ? 'positive' : 'negative'}`}>
                          {sessionStats.stackedImbalances.winRate.toFixed(1)}%
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (1m)</span>
                        <span className={`metric-value ${sessionStats.stackedImbalances.avgMove1m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.stackedImbalances.avgMove1m >= 0 ? '+' : ''}{sessionStats.stackedImbalances.avgMove1m.toFixed(2)}
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (5m)</span>
                        <span className={`metric-value ${sessionStats.stackedImbalances.avgMove5m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.stackedImbalances.avgMove5m >= 0 ? '+' : ''}{sessionStats.stackedImbalances.avgMove5m.toFixed(2)}
                        </span>
                      </div>
                    </div>
                    <div className="win-loss">
                      <span className="wins">{sessionStats.stackedImbalances.wins} W</span>
                      <span className="losses">{sessionStats.stackedImbalances.losses} L</span>
                    </div>
                  </div>

                  {/* Confluences */}
                  <div className="signal-card confluence-card">
                    <h3>Confluences</h3>
                    <div className="signal-counts">
                      <span className="count total">{sessionStats.confluences.count} total</span>
                      <span className="count bullish">{sessionStats.confluences.bullishCount} bullish</span>
                      <span className="count bearish">{sessionStats.confluences.bearishCount} bearish</span>
                    </div>
                    <div className="signal-metrics">
                      <div className="metric">
                        <span className="metric-label">Win Rate</span>
                        <span className={`metric-value ${sessionStats.confluences.winRate >= 50 ? 'positive' : 'negative'}`}>
                          {sessionStats.confluences.winRate.toFixed(1)}%
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (1m)</span>
                        <span className={`metric-value ${sessionStats.confluences.avgMove1m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.confluences.avgMove1m >= 0 ? '+' : ''}{sessionStats.confluences.avgMove1m.toFixed(2)}
                        </span>
                      </div>
                      <div className="metric">
                        <span className="metric-label">Avg Move (5m)</span>
                        <span className={`metric-value ${sessionStats.confluences.avgMove5m >= 0 ? 'positive' : 'negative'}`}>
                          {sessionStats.confluences.avgMove5m >= 0 ? '+' : ''}{sessionStats.confluences.avgMove5m.toFixed(2)}
                        </span>
                      </div>
                    </div>
                    <div className="win-loss">
                      <span className="wins">{sessionStats.confluences.wins} W</span>
                      <span className="losses">{sessionStats.confluences.losses} L</span>
                    </div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="stats-loading">
                <p>Waiting for session data...</p>
                <p className="stats-hint">Statistics will appear after signals are detected</p>
              </div>
            )}
          </div>
        )}
      </div>

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
        <div className="bubble-count">
          {selectedSymbol === 'all'
            ? `${bubbles.length} bubbles`
            : `${filteredBubbles.length}/${bubbles.length} bubbles`}
        </div>
      </footer>
    </div>
  );
}

export default App;
