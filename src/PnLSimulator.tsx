import { useState, useCallback, useMemo } from 'react';

export interface SimulatedTrade {
  id: string;
  entryTime: number;
  entryPrice: number;
  direction: 'long' | 'short';
  size: number;
  signalType: string;
  signalDetails?: string;
  exitTime?: number;
  exitPrice?: number;
  pnl?: number;
  status: 'open' | 'closed';
}

interface PnLSimulatorProps {
  isOpen: boolean;
  onClose: () => void;
  currentPrice: number | null;
  trades: SimulatedTrade[];
  onAddTrade: (trade: Omit<SimulatedTrade, 'id' | 'status'>) => void;
  onCloseTrade: (tradeId: string, exitPrice: number) => void;
  onClearTrades: () => void;
}

export function PnLSimulator({
  isOpen,
  onClose,
  currentPrice,
  trades,
  onAddTrade,
  onCloseTrade,
  onClearTrades,
}: PnLSimulatorProps) {
  const [manualEntry, setManualEntry] = useState({
    price: '',
    direction: 'long' as 'long' | 'short',
    size: '1',
    signalType: 'manual',
  });

  const openTrades = useMemo(() => trades.filter((t) => t.status === 'open'), [trades]);
  const closedTrades = useMemo(() => trades.filter((t) => t.status === 'closed'), [trades]);

  const totalPnL = useMemo(() => {
    return closedTrades.reduce((sum, t) => sum + (t.pnl || 0), 0);
  }, [closedTrades]);

  const unrealizedPnL = useMemo(() => {
    if (!currentPrice) return 0;
    return openTrades.reduce((sum, t) => {
      const priceDiff = currentPrice - t.entryPrice;
      const tradePnL = t.direction === 'long' ? priceDiff * t.size : -priceDiff * t.size;
      return sum + tradePnL;
    }, 0);
  }, [openTrades, currentPrice]);

  const winRate = useMemo(() => {
    if (closedTrades.length === 0) return 0;
    const wins = closedTrades.filter((t) => (t.pnl || 0) > 0).length;
    return (wins / closedTrades.length) * 100;
  }, [closedTrades]);

  const handleManualTrade = useCallback(() => {
    const price = parseFloat(manualEntry.price) || currentPrice;
    if (!price) return;

    onAddTrade({
      entryTime: Date.now(),
      entryPrice: price,
      direction: manualEntry.direction,
      size: parseInt(manualEntry.size, 10) || 1,
      signalType: manualEntry.signalType,
    });

    setManualEntry((prev) => ({ ...prev, price: '' }));
  }, [manualEntry, currentPrice, onAddTrade]);

  const handleCloseAllOpen = useCallback(() => {
    if (!currentPrice) return;
    openTrades.forEach((trade) => {
      onCloseTrade(trade.id, currentPrice);
    });
  }, [openTrades, currentPrice, onCloseTrade]);

  if (!isOpen) return null;

  return (
    <div className="pnl-simulator-overlay" onClick={onClose}>
      <div className="pnl-simulator-modal" onClick={(e) => e.stopPropagation()}>
        <div className="pnl-simulator-header">
          <h3>P&L Simulator</h3>
          <button className="close-modal-btn" onClick={onClose}>
            X
          </button>
        </div>

        <div className="pnl-summary">
          <div className="pnl-summary-item">
            <span className="pnl-label">Realized P&L</span>
            <span className={`pnl-value ${totalPnL >= 0 ? 'positive' : 'negative'}`}>
              {totalPnL >= 0 ? '+' : ''}
              {totalPnL.toFixed(2)}
            </span>
          </div>
          <div className="pnl-summary-item">
            <span className="pnl-label">Unrealized P&L</span>
            <span className={`pnl-value ${unrealizedPnL >= 0 ? 'positive' : 'negative'}`}>
              {unrealizedPnL >= 0 ? '+' : ''}
              {unrealizedPnL.toFixed(2)}
            </span>
          </div>
          <div className="pnl-summary-item">
            <span className="pnl-label">Win Rate</span>
            <span className={`pnl-value ${winRate >= 50 ? 'positive' : 'negative'}`}>
              {winRate.toFixed(1)}%
            </span>
          </div>
          <div className="pnl-summary-item">
            <span className="pnl-label">Trades</span>
            <span className="pnl-value">{closedTrades.length} closed / {openTrades.length} open</span>
          </div>
        </div>

        <div className="pnl-section">
          <h4>Quick Trade Entry</h4>
          <div className="trade-entry-form">
            <div className="form-row">
              <label>Direction</label>
              <div className="direction-toggle">
                <button
                  className={`direction-btn long ${manualEntry.direction === 'long' ? 'active' : ''}`}
                  onClick={() => setManualEntry((prev) => ({ ...prev, direction: 'long' }))}
                >
                  LONG
                </button>
                <button
                  className={`direction-btn short ${manualEntry.direction === 'short' ? 'active' : ''}`}
                  onClick={() => setManualEntry((prev) => ({ ...prev, direction: 'short' }))}
                >
                  SHORT
                </button>
              </div>
            </div>
            <div className="form-row">
              <label>Entry Price</label>
              <input
                type="number"
                step="0.01"
                placeholder={currentPrice?.toFixed(2) || 'Enter price'}
                value={manualEntry.price}
                onChange={(e) => setManualEntry((prev) => ({ ...prev, price: e.target.value }))}
                className="trade-input"
              />
            </div>
            <div className="form-row">
              <label>Size</label>
              <input
                type="number"
                min="1"
                value={manualEntry.size}
                onChange={(e) => setManualEntry((prev) => ({ ...prev, size: e.target.value }))}
                className="trade-input"
              />
            </div>
            <div className="form-row">
              <label>Signal Type</label>
              <select
                value={manualEntry.signalType}
                onChange={(e) => setManualEntry((prev) => ({ ...prev, signalType: e.target.value }))}
                className="trade-select"
              >
                <option value="manual">Manual</option>
                <option value="confluence">Confluence</option>
                <option value="delta_flip">Delta Flip</option>
                <option value="absorption">Absorption</option>
                <option value="stacked_imbalance">Stacked Imbalance</option>
              </select>
            </div>
            <button className="enter-trade-btn" onClick={handleManualTrade}>
              Enter Trade
            </button>
          </div>
        </div>

        {openTrades.length > 0 && (
          <div className="pnl-section">
            <div className="section-header">
              <h4>Open Positions ({openTrades.length})</h4>
              <button className="close-all-btn" onClick={handleCloseAllOpen}>
                Close All @ Market
              </button>
            </div>
            <div className="trades-list">
              {openTrades.map((trade) => {
                const unrealized = currentPrice
                  ? (trade.direction === 'long'
                      ? (currentPrice - trade.entryPrice) * trade.size
                      : (trade.entryPrice - currentPrice) * trade.size)
                  : 0;
                return (
                  <div key={trade.id} className={`trade-row open ${trade.direction}`}>
                    <div className="trade-info">
                      <span className={`trade-direction ${trade.direction}`}>
                        {trade.direction.toUpperCase()}
                      </span>
                      <span className="trade-size">{trade.size}x</span>
                      <span className="trade-entry">@ {trade.entryPrice.toFixed(2)}</span>
                      <span className="trade-signal">{trade.signalType}</span>
                    </div>
                    <div className="trade-pnl-section">
                      <span className={`trade-unrealized ${unrealized >= 0 ? 'positive' : 'negative'}`}>
                        {unrealized >= 0 ? '+' : ''}{unrealized.toFixed(2)}
                      </span>
                      <button
                        className="close-trade-btn"
                        onClick={() => currentPrice && onCloseTrade(trade.id, currentPrice)}
                        disabled={!currentPrice}
                      >
                        Close
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {closedTrades.length > 0 && (
          <div className="pnl-section">
            <div className="section-header">
              <h4>Trade History ({closedTrades.length})</h4>
              <button className="clear-trades-btn" onClick={onClearTrades}>
                Clear History
              </button>
            </div>
            <div className="trades-list history">
              {closedTrades.slice(-10).reverse().map((trade) => (
                <div key={trade.id} className={`trade-row closed ${(trade.pnl || 0) >= 0 ? 'win' : 'loss'}`}>
                  <div className="trade-info">
                    <span className={`trade-direction ${trade.direction}`}>
                      {trade.direction.toUpperCase()}
                    </span>
                    <span className="trade-size">{trade.size}x</span>
                    <span className="trade-prices">
                      {trade.entryPrice.toFixed(2)} &rarr; {trade.exitPrice?.toFixed(2)}
                    </span>
                    <span className="trade-signal">{trade.signalType}</span>
                  </div>
                  <span className={`trade-pnl ${(trade.pnl || 0) >= 0 ? 'positive' : 'negative'}`}>
                    {(trade.pnl || 0) >= 0 ? '+' : ''}{(trade.pnl || 0).toFixed(2)}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {trades.length === 0 && (
          <div className="pnl-empty">
            <p>No trades yet</p>
            <p className="pnl-hint">
              Enter trades manually above, or click signal badges to auto-enter trades
            </p>
          </div>
        )}

        <div className="pnl-footer">
          <span className="pnl-disclaimer">
            Simulated P&L for educational purposes only. Not actual trading.
          </span>
        </div>
      </div>
    </div>
  );
}
