import { useState, useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';
import {
  MarketState,
  LocationType,
  AggressionType,
  SetupGrade,
  TradeDirection,
  ExitType,
  NewTradeForm,
  CloseTradeForm,
} from './types';

export function TradeEntry() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const closeTradeId = searchParams.get('close');

  const {
    getTodaySession,
    createTrade,
    closeTrade,
    trades,
  } = useJournalStore();

  const todaySession = getTodaySession();
  const tradeToClose = closeTradeId
    ? trades.find((t) => t.id === closeTradeId)
    : null;

  // Form state for new trade
  const [newTradeForm, setNewTradeForm] = useState<NewTradeForm>({
    marketState: 'imbalance',
    locationType: 'lvn',
    locationPrice: '',
    aggressionType: 'delta_flip',
    prismConfirmation: true,
    setupGrade: 'A',
    direction: 'long',
    entryPrice: '',
    stopPrice: '',
    targetPrice: '',
    positionSize: '1',
    entryNotes: '',
  });

  // Form state for closing trade
  const [closeForm, setCloseForm] = useState<CloseTradeForm>({
    exitPrice: '',
    exitType: 'manual',
    exitNotes: '',
    whatWorked: '',
    whatToImprove: '',
  });

  // Pre-fill close form with entry price if closing
  useEffect(() => {
    if (tradeToClose) {
      setCloseForm((prev) => ({
        ...prev,
        exitPrice: tradeToClose.entryPrice.toString(),
      }));
    }
  }, [tradeToClose]);

  // Calculate R:R as user types
  const calculateRR = () => {
    const entry = parseFloat(newTradeForm.entryPrice);
    const stop = parseFloat(newTradeForm.stopPrice);
    const target = parseFloat(newTradeForm.targetPrice);

    if (isNaN(entry) || isNaN(stop) || isNaN(target)) return null;

    const risk = Math.abs(entry - stop);
    const reward = Math.abs(target - entry);
    return risk > 0 ? (reward / risk).toFixed(2) : null;
  };

  // Calculate potential P&L
  const calculatePotentialPnL = () => {
    const entry = parseFloat(newTradeForm.entryPrice);
    const target = parseFloat(newTradeForm.targetPrice);
    const stop = parseFloat(newTradeForm.stopPrice);
    const size = parseInt(newTradeForm.positionSize) || 1;
    const pointValue = 20; // NQ = $20/point

    if (isNaN(entry) || isNaN(target) || isNaN(stop)) return null;

    const isLong = newTradeForm.direction === 'long';
    const targetPnl = isLong
      ? (target - entry) * pointValue * size
      : (entry - target) * pointValue * size;
    const stopPnl = isLong
      ? (stop - entry) * pointValue * size
      : (entry - stop) * pointValue * size;

    return { targetPnl, stopPnl };
  };

  const handleNewTrade = (e: React.FormEvent) => {
    e.preventDefault();

    if (!todaySession) {
      alert('Please start a session first');
      navigate('/journal/session');
      return;
    }

    createTrade(todaySession.id, newTradeForm);
    navigate('/journal');
  };

  const handleCloseTrade = (e: React.FormEvent) => {
    e.preventDefault();

    if (!tradeToClose) return;

    closeTrade(tradeToClose.id, closeForm);
    navigate('/journal');
  };

  const rr = calculateRR();
  const potentialPnl = calculatePotentialPnL();

  // If closing a trade
  if (tradeToClose) {
    return (
      <div className="max-w-2xl mx-auto">
        <div className="card">
          <h2 className="text-xl font-bold text-white mb-6">Close Trade</h2>

          {/* Trade summary */}
          <div className="p-4 bg-bg-tertiary rounded-lg mb-6">
            <div className="flex items-center justify-between mb-3">
              <span
                className={`px-3 py-1 rounded font-bold ${
                  tradeToClose.direction === 'long'
                    ? 'bg-green-500/20 text-green-500'
                    : 'bg-red-500/20 text-red-500'
                }`}
              >
                {tradeToClose.direction.toUpperCase()}
              </span>
              <span className="text-white/50">
                {tradeToClose.locationType.toUpperCase()} •{' '}
                {tradeToClose.aggressionType.replace('_', ' ')}
              </span>
            </div>
            <div className="grid grid-cols-3 gap-4 text-center">
              <div>
                <div className="text-xs text-white/50">Entry</div>
                <div className="font-mono text-white">
                  {tradeToClose.entryPrice.toFixed(2)}
                </div>
              </div>
              <div>
                <div className="text-xs text-white/50">Stop</div>
                <div className="font-mono text-red-500">
                  {tradeToClose.stopPrice.toFixed(2)}
                </div>
              </div>
              <div>
                <div className="text-xs text-white/50">Target</div>
                <div className="font-mono text-green-500">
                  {tradeToClose.targetPrice.toFixed(2)}
                </div>
              </div>
            </div>
          </div>

          <form onSubmit={handleCloseTrade} className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="label">Exit Price</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={closeForm.exitPrice}
                  onChange={(e) =>
                    setCloseForm({ ...closeForm, exitPrice: e.target.value })
                  }
                  required
                />
              </div>
              <div>
                <label className="label">Exit Type</label>
                <select
                  className="select"
                  value={closeForm.exitType}
                  onChange={(e) =>
                    setCloseForm({
                      ...closeForm,
                      exitType: e.target.value as ExitType,
                    })
                  }
                >
                  <option value="target">Target</option>
                  <option value="stop">Stop</option>
                  <option value="trailing">Trailing Stop</option>
                  <option value="manual">Manual</option>
                  <option value="scratch">Scratch</option>
                </select>
              </div>
            </div>

            <div>
              <label className="label">Exit Notes</label>
              <textarea
                className="input min-h-[80px]"
                value={closeForm.exitNotes}
                onChange={(e) =>
                  setCloseForm({ ...closeForm, exitNotes: e.target.value })
                }
                placeholder="What happened? Why did you exit here?"
              />
            </div>

            <div>
              <label className="label">What Worked</label>
              <textarea
                className="input min-h-[60px]"
                value={closeForm.whatWorked}
                onChange={(e) =>
                  setCloseForm({ ...closeForm, whatWorked: e.target.value })
                }
                placeholder="What did you do well?"
              />
            </div>

            <div>
              <label className="label">What to Improve</label>
              <textarea
                className="input min-h-[60px]"
                value={closeForm.whatToImprove}
                onChange={(e) =>
                  setCloseForm({ ...closeForm, whatToImprove: e.target.value })
                }
                placeholder="What would you do differently?"
              />
            </div>

            <div className="flex gap-3 pt-4">
              <button
                type="button"
                onClick={() => navigate('/journal')}
                className="btn-ghost flex-1"
              >
                Cancel
              </button>
              <button type="submit" className="btn-primary flex-1">
                Close Trade
              </button>
            </div>
          </form>
        </div>
      </div>
    );
  }

  // New trade form
  return (
    <div className="max-w-4xl mx-auto">
      <div className="card">
        <h2 className="text-xl font-bold text-white mb-6">New Trade Entry</h2>

        {!todaySession && (
          <div className="p-4 bg-yellow-500/20 text-yellow-500 rounded-lg mb-6">
            No session started for today.{' '}
            <button
              onClick={() => navigate('/journal/session')}
              className="underline hover:no-underline"
            >
              Start a session first
            </button>
          </div>
        )}

        <form onSubmit={handleNewTrade} className="space-y-6">
          {/* Three Elements Section */}
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider">
              Three Elements
            </h3>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div>
                <label className="label">Market State</label>
                <select
                  className="select"
                  value={newTradeForm.marketState}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      marketState: e.target.value as MarketState,
                    })
                  }
                >
                  <option value="balance">Balance (Mean Reversion)</option>
                  <option value="imbalance">Imbalance (Trend)</option>
                </select>
              </div>

              <div>
                <label className="label">Location Type</label>
                <select
                  className="select"
                  value={newTradeForm.locationType}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      locationType: e.target.value as LocationType,
                    })
                  }
                >
                  <option value="lvn">LVN</option>
                  <option value="poc">POC</option>
                  <option value="vah">VAH</option>
                  <option value="val">VAL</option>
                  <option value="vwap">VWAP</option>
                  <option value="vwap_upper">VWAP Upper</option>
                  <option value="vwap_lower">VWAP Lower</option>
                  <option value="pdh">PDH</option>
                  <option value="pdl">PDL</option>
                  <option value="onh">ONH</option>
                  <option value="onl">ONL</option>
                  <option value="other">Other</option>
                </select>
              </div>

              <div>
                <label className="label">Aggression Type</label>
                <select
                  className="select"
                  value={newTradeForm.aggressionType}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      aggressionType: e.target.value as AggressionType,
                    })
                  }
                >
                  <option value="delta_flip">Delta Flip</option>
                  <option value="absorption">Absorption</option>
                  <option value="stacked_imbalance">Stacked Imbalance</option>
                  <option value="big_prints">Big Prints</option>
                  <option value="none">None</option>
                </select>
              </div>

              <div>
                <label className="label">Setup Grade</label>
                <select
                  className="select"
                  value={newTradeForm.setupGrade}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      setupGrade: e.target.value as SetupGrade,
                    })
                  }
                >
                  <option value="A">A - Perfect Setup</option>
                  <option value="B">B - Good Setup</option>
                  <option value="C">C - Marginal Setup</option>
                </select>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="label">Location Price</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={newTradeForm.locationPrice}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      locationPrice: e.target.value,
                    })
                  }
                  placeholder="Price of the key level"
                  required
                />
              </div>
              <div className="flex items-center pt-6">
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    className="w-5 h-5 rounded bg-bg-tertiary border-border"
                    checked={newTradeForm.prismConfirmation}
                    onChange={(e) =>
                      setNewTradeForm({
                        ...newTradeForm,
                        prismConfirmation: e.target.checked,
                      })
                    }
                  />
                  <span className="text-white">PRISM Confirmation</span>
                </label>
              </div>
            </div>
          </div>

          {/* Execution Section */}
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider">
              Execution
            </h3>

            <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
              <div>
                <label className="label">Direction</label>
                <select
                  className="select"
                  value={newTradeForm.direction}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      direction: e.target.value as TradeDirection,
                    })
                  }
                >
                  <option value="long">Long</option>
                  <option value="short">Short</option>
                </select>
              </div>

              <div>
                <label className="label">Entry Price</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={newTradeForm.entryPrice}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      entryPrice: e.target.value,
                    })
                  }
                  required
                />
              </div>

              <div>
                <label className="label">Stop Price</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={newTradeForm.stopPrice}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      stopPrice: e.target.value,
                    })
                  }
                  required
                />
              </div>

              <div>
                <label className="label">Target Price</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={newTradeForm.targetPrice}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      targetPrice: e.target.value,
                    })
                  }
                  required
                />
              </div>

              <div>
                <label className="label">Contracts</label>
                <input
                  type="number"
                  min="1"
                  className="input"
                  value={newTradeForm.positionSize}
                  onChange={(e) =>
                    setNewTradeForm({
                      ...newTradeForm,
                      positionSize: e.target.value,
                    })
                  }
                  required
                />
              </div>
            </div>

            {/* Calculated metrics */}
            {rr && potentialPnl && (
              <div className="grid grid-cols-3 gap-4 p-4 bg-bg-tertiary rounded-lg">
                <div className="text-center">
                  <div className="text-xs text-white/50">R:R Ratio</div>
                  <div
                    className={`text-xl font-bold font-mono ${
                      parseFloat(rr) >= 2 ? 'text-green-500' : 'text-yellow-500'
                    }`}
                  >
                    {rr}:1
                  </div>
                </div>
                <div className="text-center">
                  <div className="text-xs text-white/50">Max Profit</div>
                  <div className="text-xl font-bold font-mono text-green-500">
                    +${potentialPnl.targetPnl.toFixed(0)}
                  </div>
                </div>
                <div className="text-center">
                  <div className="text-xs text-white/50">Max Loss</div>
                  <div className="text-xl font-bold font-mono text-red-500">
                    ${potentialPnl.stopPnl.toFixed(0)}
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Notes Section */}
          <div>
            <label className="label">Entry Notes</label>
            <textarea
              className="input min-h-[100px]"
              value={newTradeForm.entryNotes}
              onChange={(e) =>
                setNewTradeForm({ ...newTradeForm, entryNotes: e.target.value })
              }
              placeholder="Why are you taking this trade? What's the thesis?"
            />
          </div>

          <div className="flex gap-3 pt-4">
            <button
              type="button"
              onClick={() => navigate('/journal')}
              className="btn-ghost flex-1"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn-success flex-1"
              disabled={!todaySession}
            >
              Log Trade
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
