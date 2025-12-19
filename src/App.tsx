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
}

// Update these for current front-month contracts
// H=Mar, M=Jun, U=Sep, Z=Dec
const SYMBOLS = ['NQH5', 'ESH5'] as const;
type Symbol = typeof SYMBOLS[number];

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

  // Handle incoming trades
  const handleTrade = useCallback((trade: Trade) => {
    if (trade.size < minBubbleSize) return;

    const newBubble: Bubble = {
      id: `bubble-${bubbleIdRef.current++}`,
      price: trade.price,
      size: trade.size,
      side: trade.side,
      timestamp: Date.now(),
      x: 1, // Start from right edge
      opacity: 1
    };

    setBubbles(prev => {
      const updated = [...prev, newBubble];
      // Keep last 500 bubbles max for performance
      if (updated.length > 500) {
        return updated.slice(-500);
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
  }, [minBubbleSize]);

  // Animation loop to move bubbles left and fade out
  useEffect(() => {
    const interval = setInterval(() => {
      setBubbles(prev => {
        return prev
          .map(bubble => ({
            ...bubble,
            x: bubble.x - 0.002, // Move left
            opacity: Math.max(0, bubble.opacity - 0.002) // Fade out
          }))
          .filter(bubble => bubble.x > -0.1 && bubble.opacity > 0);
      });
    }, 16); // ~60fps

    return () => clearInterval(interval);
  }, []);

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
        </div>
      </header>

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
          <BubbleRenderer
            bubbles={bubbles}
            priceRange={priceRange}
            canvasRef={canvasRef}
          />
          <button className="disconnect-btn" onClick={disconnect}>
            DISCONNECT
          </button>
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
        <div className="bubble-count">
          {bubbles.length} bubbles
        </div>
      </footer>
    </div>
  );
}

export default App;
