import { useNavigate } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';
import { format } from 'date-fns';

export function JournalDashboard() {
  const navigate = useNavigate();
  const {
    getTodaySession,
    getTradesForSession,
    getOpenTrades,
    getAllTimeStats,
    getReadyToFundChecklist,
    sessions,
  } = useJournalStore();

  const todaySession = getTodaySession();
  const todayTrades = todaySession ? getTradesForSession(todaySession.id) : [];
  const openTrades = getOpenTrades();
  const stats = getAllTimeStats();
  const checklist = getReadyToFundChecklist();

  // Recent sessions (last 5)
  const recentSessions = [...sessions]
    .sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime())
    .slice(0, 5);

  return (
    <div className="max-w-7xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white">Trading Journal</h1>
          <p className="text-white/50 mt-1">
            {format(new Date(), 'EEEE, MMMM d, yyyy')}
          </p>
        </div>
        <div className="flex gap-3">
          {!todaySession && (
            <button
              onClick={() => navigate('/journal/session')}
              className="btn btn-primary"
            >
              + Start Today's Session
            </button>
          )}
          {todaySession && (
            <button onClick={() => navigate('/journal/trade')} className="btn btn-success">
              + New Trade
            </button>
          )}
        </div>
      </div>

      {/* Quick Stats Row - First 4 cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4" style={{ marginBottom: '24px' }}>
        <StatCard
          label="Total Trades"
          value={stats.totalTrades.toString()}
          color="white"
        />
        <StatCard
          label="Win Rate"
          value={`${stats.winRate.toFixed(1)}%`}
          color={stats.winRate >= 50 ? 'green' : 'red'}
        />
        <StatCard
          label="Profit Factor"
          value={stats.profitFactor === Infinity ? '∞' : stats.profitFactor.toFixed(2)}
          color={stats.profitFactor >= 1.5 ? 'green' : stats.profitFactor >= 1 ? 'yellow' : 'red'}
        />
        <StatCard
          label="Net P&L"
          value={`${stats.netPnl >= 0 ? '+' : ''}$${stats.netPnl.toFixed(0)}`}
          color={stats.netPnl >= 0 ? 'green' : 'red'}
        />
      </div>

      {/* Second Row - Avg Winner, Avg Loser, Ready to Fund */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4" style={{ marginBottom: '24px' }}>
        <StatCard
          label="Avg Winner"
          value={`$${stats.avgWinner.toFixed(0)}`}
          color="green"
        />
        <StatCard
          label="Avg Loser"
          value={`$${stats.avgLoser.toFixed(0)}`}
          color="red"
        />
        {/* Ready to Fund Checklist - spans 2 columns */}
        <div className="col-span-2 card">
          <h2 className="text-lg font-semibold text-white mb-4">Ready to Fund?</h2>
          <div className="grid grid-cols-5 gap-4">
            <ChecklistItem
              label="50+ Trades"
              current={checklist.minTrades.current}
              required={checklist.minTrades.required}
              passed={checklist.minTrades.passed}
              format="number"
            />
            <ChecklistItem
              label="45%+ WR"
              current={checklist.winRate.current}
              required={checklist.winRate.required}
              passed={checklist.winRate.passed}
              format="percent"
            />
            <ChecklistItem
              label="1.3+ PF"
              current={checklist.profitFactor.current}
              required={checklist.profitFactor.required}
              passed={checklist.profitFactor.passed}
              format="decimal"
            />
            <ChecklistItem
              label="Max Loss < $75"
              current={Math.abs(checklist.maxSingleLoss.current)}
              required={Math.abs(checklist.maxSingleLoss.required)}
              passed={checklist.maxSingleLoss.passed}
              format="currency"
              invert
            />
            <ChecklistItem
              label="Daily < $150"
              current={Math.abs(checklist.maxDailyLoss.current)}
              required={Math.abs(checklist.maxDailyLoss.required)}
              passed={checklist.maxDailyLoss.passed}
              format="currency"
              invert
            />
          </div>
          <div
            className={`mt-5 py-2 rounded text-center text-sm font-medium ${
              checklist.allPassed
                ? 'bg-green-500/20 text-green-500'
                : 'bg-white/10 text-white/50'
            }`}
          >
            {checklist.allPassed
              ? '✓ Ready for Prop Firm Evaluation!'
              : 'Keep trading to meet all criteria'}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Today's Session */}
        <div className="lg:col-span-2 space-y-6">
          {/* Today's Session Card */}
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-white">Today's Session</h2>
              {todaySession && (
                <button
                  onClick={() => navigate('/journal/session')}
                  className="text-sm text-blue-500 hover:text-blue-400"
                >
                  Edit Session →
                </button>
              )}
            </div>

            {todaySession ? (
              <div className="space-y-4">
                {/* Session levels */}
                <div className="grid grid-cols-4 gap-3 text-sm">
                  <LevelBadge label="PDH" value={todaySession.pdh} />
                  <LevelBadge label="PDL" value={todaySession.pdl} />
                  <LevelBadge label="ONH" value={todaySession.onh} />
                  <LevelBadge label="ONL" value={todaySession.onl} />
                </div>

                {/* Thesis */}
                {todaySession.dailyThesis && (
                  <div className="p-3 bg-bg-tertiary rounded-lg">
                    <p className="text-sm text-white/70">
                      <span className="text-white/50">Thesis: </span>
                      {todaySession.dailyThesis}
                    </p>
                  </div>
                )}

                {/* Session stats */}
                <div className="grid grid-cols-4 gap-3">
                  <div className="text-center">
                    <div className="text-2xl font-bold font-mono text-white">
                      {todaySession.totalTrades}
                    </div>
                    <div className="text-xs text-white/50">Trades</div>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold font-mono text-green-500">
                      {todaySession.winners}
                    </div>
                    <div className="text-xs text-white/50">Winners</div>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold font-mono text-red-500">
                      {todaySession.losers}
                    </div>
                    <div className="text-xs text-white/50">Losers</div>
                  </div>
                  <div className="text-center">
                    <div
                      className={`text-2xl font-bold font-mono ${
                        todaySession.netPnl >= 0 ? 'text-green-500' : 'text-red-500'
                      }`}
                    >
                      {todaySession.netPnl >= 0 ? '+' : ''}${todaySession.netPnl.toFixed(0)}
                    </div>
                    <div className="text-xs text-white/50">P&L</div>
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-center py-8">
                <p className="text-white/50 mb-4">No session started for today</p>
                <button
                  onClick={() => navigate('/journal/session')}
                  className="btn btn-primary"
                >
                  Start Today's Session
                </button>
              </div>
            )}
          </div>

          {/* Open Trades */}
          {openTrades.length > 0 && (
            <div className="card">
              <h2 className="text-lg font-semibold text-white mb-4">Open Trades</h2>
              <div className="space-y-2">
                {openTrades.map((trade) => (
                  <div
                    key={trade.id}
                    className="flex items-center justify-between p-3 bg-bg-tertiary rounded-lg"
                  >
                    <div className="flex items-center gap-3">
                      <span
                        className={`px-2 py-1 rounded text-xs font-bold ${
                          trade.direction === 'long'
                            ? 'bg-green-500/20 text-green-500'
                            : 'bg-red-500/20 text-red-500'
                        }`}
                      >
                        {trade.direction.toUpperCase()}
                      </span>
                      <span className="font-mono text-white">
                        @ {trade.entryPrice.toFixed(2)}
                      </span>
                      <span className="text-white/50 text-sm">
                        {trade.locationType.toUpperCase()} • {trade.aggressionType.replace('_', ' ')}
                      </span>
                    </div>
                    <div className="flex items-center gap-4">
                      <div className="text-right text-sm">
                        <div className="text-red-500">SL: {trade.stopPrice.toFixed(2)}</div>
                        <div className="text-green-500">TP: {trade.targetPrice.toFixed(2)}</div>
                      </div>
                      <button
                        onClick={() => navigate(`/journal/trade?close=${trade.id}`)}
                        className="btn-ghost text-sm"
                      >
                        Close
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Today's Trades */}
          {todayTrades.length > 0 && (
            <div className="card">
              <h2 className="text-lg font-semibold text-white mb-4">Today's Trades</h2>
              <div className="overflow-x-auto">
                <table className="table">
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>Time</th>
                      <th>Direction</th>
                      <th>Entry</th>
                      <th>Exit</th>
                      <th>P&L</th>
                      <th>Grade</th>
                    </tr>
                  </thead>
                  <tbody>
                    {todayTrades.map((trade) => (
                      <tr key={trade.id}>
                        <td className="font-mono">{trade.tradeNumber}</td>
                        <td className="text-white/70">
                          {format(new Date(trade.entryTime), 'HH:mm')}
                        </td>
                        <td>
                          <span
                            className={`badge ${
                              trade.direction === 'long' ? 'badge-green' : 'badge-red'
                            }`}
                          >
                            {trade.direction.toUpperCase()}
                          </span>
                        </td>
                        <td className="font-mono">{trade.entryPrice.toFixed(2)}</td>
                        <td className="font-mono">
                          {trade.exitPrice?.toFixed(2) ?? '-'}
                        </td>
                        <td>
                          {trade.pnl !== null ? (
                            <span
                              className={`font-mono font-bold ${
                                trade.pnl >= 0 ? 'text-green-500' : 'text-red-500'
                              }`}
                            >
                              {trade.pnl >= 0 ? '+' : ''}${trade.pnl.toFixed(0)}
                            </span>
                          ) : (
                            <span className="text-yellow-500">Open</span>
                          )}
                        </td>
                        <td>
                          <span
                            className={`badge ${
                              trade.setupGrade === 'A'
                                ? 'badge-green'
                                : trade.setupGrade === 'B'
                                ? 'badge-blue'
                                : 'bg-white/10 text-white/70'
                            }`}
                          >
                            {trade.setupGrade}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>

        {/* Right sidebar */}
        <div className="space-y-6">
          {/* Recent Sessions */}
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-white">Recent Sessions</h2>
              <button
                onClick={() => navigate('/journal/history')}
                className="text-sm text-blue-500 hover:text-blue-400"
              >
                View All →
              </button>
            </div>
            <div className="space-y-2">
              {recentSessions.length > 0 ? (
                recentSessions.map((session) => (
                  <div
                    key={session.id}
                    className="flex items-center justify-between p-3 bg-bg-tertiary rounded-lg"
                  >
                    <div>
                      <div className="text-sm font-medium text-white">
                        {format(new Date(session.date), 'MMM d, yyyy')}
                      </div>
                      <div className="text-xs text-white/50">
                        {session.totalTrades} trades
                      </div>
                    </div>
                    <div
                      className={`font-mono font-bold ${
                        session.netPnl >= 0 ? 'text-green-500' : 'text-red-500'
                      }`}
                    >
                      {session.netPnl >= 0 ? '+' : ''}${session.netPnl.toFixed(0)}
                    </div>
                  </div>
                ))
              ) : (
                <p className="text-white/50 text-sm text-center py-4">
                  No sessions yet
                </p>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// Helper components
function StatCard({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: 'green' | 'red' | 'yellow' | 'white';
}) {
  const colorClasses = {
    green: 'text-green-500',
    red: 'text-red-500',
    yellow: 'text-yellow-500',
    white: 'text-white',
  };

  return (
    <div className="card text-center">
      <div className={`text-2xl font-bold font-mono ${colorClasses[color]}`}>{value}</div>
      <div className="text-xs text-white/50 mt-1">{label}</div>
    </div>
  );
}

function LevelBadge({ label, value }: { label: string; value: number | null }) {
  return (
    <div className="text-center p-2 bg-bg-tertiary rounded-lg">
      <div className="text-xs text-white/50">{label}</div>
      <div className="font-mono text-white">{value?.toFixed(2) ?? '-'}</div>
    </div>
  );
}

function ChecklistItem({
  label,
  current,
  required,
  passed,
  format,
  invert = false,
}: {
  label: string;
  current: number;
  required: number;
  passed: boolean;
  format: 'number' | 'percent' | 'decimal' | 'currency';
  invert?: boolean;
}) {
  const formatValue = (val: number) => {
    switch (format) {
      case 'percent':
        return `${val.toFixed(1)}%`;
      case 'decimal':
        return val.toFixed(2);
      case 'currency':
        return `$${val.toFixed(0)}`;
      default:
        return val.toString();
    }
  };

  const progress = invert
    ? Math.min(100, ((required - current) / required) * 100)
    : Math.min(100, (current / required) * 100);

  return (
    <div className="text-center">
      <div className="text-xs text-white/50 mb-1 truncate">{label}</div>
      <div className={`text-sm font-mono font-bold ${passed ? 'text-green-500' : 'text-white/70'}`}>
        {passed ? '✓' : formatValue(current)}
      </div>
      <div className="h-1.5 bg-bg-tertiary rounded-full overflow-hidden mt-1">
        <div
          className={`h-full rounded-full transition-all ${
            passed ? 'bg-green-500' : 'bg-blue-500'
          }`}
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}
