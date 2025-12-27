import {
  AreaChart,
  Area,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Cell,
  PieChart,
  Pie,
} from 'recharts';
import { useJournalStore } from '../stores/journalStore';

export function AnalyticsDashboard() {
  const {
    getAllTimeStats,
    getEquityCurve,
    getStatsByLocation,
    getStatsByAggression,
    getStatsByGrade,
    getStatsByMarketState,
    getReadyToFundChecklist,
    trades,
  } = useJournalStore();

  const stats = getAllTimeStats();
  const equityCurve = getEquityCurve();
  const locationStats = getStatsByLocation();
  const aggressionStats = getStatsByAggression();
  const gradeStats = getStatsByGrade();
  const marketStateStats = getStatsByMarketState();
  const checklist = getReadyToFundChecklist();

  // Prepare data for charts
  const locationData = Object.entries(locationStats)
    .filter(([_, s]) => s.count > 0)
    .map(([loc, s]) => ({
      name: loc.toUpperCase().replace('_', ' '),
      winRate: s.winRate,
      count: s.count,
      pnl: s.netPnl,
    }))
    .sort((a, b) => b.count - a.count);

  const aggressionData = Object.entries(aggressionStats)
    .filter(([_, s]) => s.count > 0)
    .map(([type, s]) => ({
      name: type.replace('_', ' ').toUpperCase(),
      winRate: s.winRate,
      count: s.count,
      pnl: s.netPnl,
    }))
    .sort((a, b) => b.count - a.count);

  const gradeData = Object.entries(gradeStats)
    .filter(([_, s]) => s.count > 0)
    .map(([grade, s]) => ({
      name: `Grade ${grade}`,
      winRate: s.winRate,
      count: s.count,
      pnl: s.netPnl,
    }));

  const marketStateData = Object.entries(marketStateStats)
    .filter(([_, s]) => s.count > 0)
    .map(([state, s]) => ({
      name: state.charAt(0).toUpperCase() + state.slice(1),
      winRate: s.winRate,
      count: s.count,
      pnl: s.netPnl,
    }));

  // Direction breakdown
  const longTrades = trades.filter((t) => t.direction === 'long' && !t.isOpen);
  const shortTrades = trades.filter((t) => t.direction === 'short' && !t.isOpen);
  const longWins = longTrades.filter((t) => (t.pnl ?? 0) > 0).length;
  const shortWins = shortTrades.filter((t) => (t.pnl ?? 0) > 0).length;

  const directionData = [
    {
      name: 'Long',
      value: longTrades.length,
      winRate: longTrades.length > 0 ? (longWins / longTrades.length) * 100 : 0,
    },
    {
      name: 'Short',
      value: shortTrades.length,
      winRate: shortTrades.length > 0 ? (shortWins / shortTrades.length) * 100 : 0,
    },
  ];

  const COLORS = ['#00e676', '#ff5252'];

  return (
    <div className="max-w-7xl mx-auto space-y-6">
      <h1 className="text-2xl font-bold text-white">Analytics Dashboard</h1>

      {trades.length === 0 ? (
        <div className="card text-center py-12">
          <p className="text-white/50 text-lg">No trades yet</p>
          <p className="text-white/30 mt-2">
            Start trading to see your analytics
          </p>
        </div>
      ) : (
        <>
          {/* Key Metrics Row */}
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-4">
            <MetricCard
              label="Total Trades"
              value={stats.totalTrades.toString()}
            />
            <MetricCard
              label="Win Rate"
              value={`${stats.winRate.toFixed(1)}%`}
              color={stats.winRate >= 50 ? 'green' : 'red'}
            />
            <MetricCard
              label="Profit Factor"
              value={
                stats.profitFactor === Infinity
                  ? '∞'
                  : stats.profitFactor.toFixed(2)
              }
              color={stats.profitFactor >= 1.5 ? 'green' : 'yellow'}
            />
            <MetricCard
              label="Net P&L"
              value={`${stats.netPnl >= 0 ? '+' : ''}$${stats.netPnl.toFixed(0)}`}
              color={stats.netPnl >= 0 ? 'green' : 'red'}
            />
            <MetricCard
              label="Avg Winner"
              value={`$${stats.avgWinner.toFixed(0)}`}
              color="green"
            />
            <MetricCard
              label="Avg Loser"
              value={`$${stats.avgLoser.toFixed(0)}`}
              color="red"
            />
            <MetricCard
              label="Avg R:R"
              value={stats.avgRR.toFixed(2)}
              color={stats.avgRR >= 1.5 ? 'green' : 'yellow'}
            />
            <MetricCard
              label="Sharpe"
              value={stats.sharpeRatio.toFixed(2)}
              color={stats.sharpeRatio >= 2 ? 'green' : 'yellow'}
            />
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Equity Curve */}
            <div className="card lg:col-span-2">
              <h3 className="text-lg font-semibold text-white mb-4">
                Equity Curve
              </h3>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={equityCurve}>
                    <defs>
                      <linearGradient id="equityGradient" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="#00e676" stopOpacity={0.3} />
                        <stop offset="95%" stopColor="#00e676" stopOpacity={0} />
                      </linearGradient>
                      <linearGradient id="drawdownGradient" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stopColor="#ff5252" stopOpacity={0.3} />
                        <stop offset="95%" stopColor="#ff5252" stopOpacity={0} />
                      </linearGradient>
                    </defs>
                    <CartesianGrid strokeDasharray="3 3" stroke="#333" />
                    <XAxis
                      dataKey="date"
                      stroke="#666"
                      tick={{ fill: '#666', fontSize: 12 }}
                    />
                    <YAxis stroke="#666" tick={{ fill: '#666', fontSize: 12 }} />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: '#1a1a1a',
                        border: '1px solid #333',
                        borderRadius: '8px',
                      }}
                      labelStyle={{ color: '#fff' }}
                    />
                    <Area
                      type="monotone"
                      dataKey="equity"
                      stroke="#00e676"
                      fill="url(#equityGradient)"
                      strokeWidth={2}
                    />
                    <Area
                      type="monotone"
                      dataKey="drawdown"
                      stroke="#ff5252"
                      fill="url(#drawdownGradient)"
                      strokeWidth={1}
                    />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Win Rate by Location */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Win Rate by Location
              </h3>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={locationData} layout="vertical">
                    <CartesianGrid strokeDasharray="3 3" stroke="#333" />
                    <XAxis
                      type="number"
                      domain={[0, 100]}
                      stroke="#666"
                      tick={{ fill: '#666', fontSize: 12 }}
                    />
                    <YAxis
                      type="category"
                      dataKey="name"
                      stroke="#666"
                      tick={{ fill: '#666', fontSize: 11 }}
                      width={80}
                    />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: '#1a1a1a',
                        border: '1px solid #333',
                        borderRadius: '8px',
                      }}
                      formatter={(value) => [`${Number(value ?? 0).toFixed(1)}%`, 'Win Rate']}
                    />
                    <Bar dataKey="winRate" radius={[0, 4, 4, 0]}>
                      {locationData.map((entry, index) => (
                        <Cell
                          key={`cell-${index}`}
                          fill={entry.winRate >= 50 ? '#00e676' : '#ff5252'}
                        />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Win Rate by Aggression */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Win Rate by Aggression Type
              </h3>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={aggressionData} layout="vertical">
                    <CartesianGrid strokeDasharray="3 3" stroke="#333" />
                    <XAxis
                      type="number"
                      domain={[0, 100]}
                      stroke="#666"
                      tick={{ fill: '#666', fontSize: 12 }}
                    />
                    <YAxis
                      type="category"
                      dataKey="name"
                      stroke="#666"
                      tick={{ fill: '#666', fontSize: 11 }}
                      width={120}
                    />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: '#1a1a1a',
                        border: '1px solid #333',
                        borderRadius: '8px',
                      }}
                      formatter={(value) => [`${Number(value ?? 0).toFixed(1)}%`, 'Win Rate']}
                    />
                    <Bar dataKey="winRate" radius={[0, 4, 4, 0]}>
                      {aggressionData.map((entry, index) => (
                        <Cell
                          key={`cell-${index}`}
                          fill={entry.winRate >= 50 ? '#00e676' : '#ff5252'}
                        />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>

            {/* Win Rate by Grade */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Win Rate by Setup Grade
              </h3>
              <div className="space-y-4">
                {gradeData.map((grade) => (
                  <div key={grade.name}>
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-white">{grade.name}</span>
                      <span className="text-white/50 text-sm">
                        {grade.count} trades
                      </span>
                    </div>
                    <div className="flex items-center gap-3">
                      <div className="flex-1 h-4 bg-bg-tertiary rounded-full overflow-hidden">
                        <div
                          className={`h-full rounded-full ${
                            grade.winRate >= 60
                              ? 'bg-green-500'
                              : grade.winRate >= 50
                              ? 'bg-yellow-500'
                              : 'bg-red-500'
                          }`}
                          style={{ width: `${grade.winRate}%` }}
                        />
                      </div>
                      <span
                        className={`font-mono font-bold w-16 text-right ${
                          grade.winRate >= 50 ? 'text-green-500' : 'text-red-500'
                        }`}
                      >
                        {grade.winRate.toFixed(1)}%
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Market State Performance */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Market State Performance
              </h3>
              <div className="space-y-4">
                {marketStateData.map((state) => (
                  <div key={state.name} className="p-4 bg-bg-tertiary rounded-lg">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-white font-medium">{state.name}</span>
                      <span
                        className={`font-mono font-bold ${
                          state.pnl >= 0 ? 'text-green-500' : 'text-red-500'
                        }`}
                      >
                        {state.pnl >= 0 ? '+' : ''}${state.pnl.toFixed(0)}
                      </span>
                    </div>
                    <div className="grid grid-cols-2 gap-4 text-sm">
                      <div>
                        <span className="text-white/50">Trades: </span>
                        <span className="text-white">{state.count}</span>
                      </div>
                      <div>
                        <span className="text-white/50">Win Rate: </span>
                        <span
                          className={
                            state.winRate >= 50 ? 'text-green-500' : 'text-red-500'
                          }
                        >
                          {state.winRate.toFixed(1)}%
                        </span>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Direction Split */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Long vs Short
              </h3>
              <div className="flex items-center gap-6">
                <div className="w-32 h-32">
                  <ResponsiveContainer width="100%" height="100%">
                    <PieChart>
                      <Pie
                        data={directionData}
                        cx="50%"
                        cy="50%"
                        innerRadius={25}
                        outerRadius={50}
                        paddingAngle={5}
                        dataKey="value"
                      >
                        {directionData.map((_, index) => (
                          <Cell key={`cell-${index}`} fill={COLORS[index]} />
                        ))}
                      </Pie>
                    </PieChart>
                  </ResponsiveContainer>
                </div>
                <div className="flex-1 space-y-3">
                  {directionData.map((d, i) => (
                    <div key={d.name} className="flex items-center gap-3">
                      <div
                        className="w-3 h-3 rounded-full"
                        style={{ backgroundColor: COLORS[i] }}
                      />
                      <span className="text-white flex-1">{d.name}</span>
                      <span className="text-white/50">{d.value} trades</span>
                      <span
                        className={`font-mono ${
                          d.winRate >= 50 ? 'text-green-500' : 'text-red-500'
                        }`}
                      >
                        {d.winRate.toFixed(1)}%
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>

            {/* Ready to Fund */}
            <div className="card">
              <h3 className="text-lg font-semibold text-white mb-4">
                Ready to Fund Checklist
              </h3>
              <div className="space-y-3">
                <CheckItem
                  label="50+ Trades"
                  current={checklist.minTrades.current}
                  passed={checklist.minTrades.passed}
                />
                <CheckItem
                  label="45%+ Win Rate"
                  current={checklist.winRate.current}
                  passed={checklist.winRate.passed}
                  format="percent"
                />
                <CheckItem
                  label="1.3+ Profit Factor"
                  current={checklist.profitFactor.current}
                  passed={checklist.profitFactor.passed}
                  format="decimal"
                />
                <CheckItem
                  label="Max Loss < $75"
                  current={Math.abs(checklist.maxSingleLoss.current)}
                  passed={checklist.maxSingleLoss.passed}
                  format="currency"
                />
                <CheckItem
                  label="Daily Loss < $150"
                  current={Math.abs(checklist.maxDailyLoss.current)}
                  passed={checklist.maxDailyLoss.passed}
                  format="currency"
                />
              </div>
              <div
                className={`mt-4 p-3 rounded-lg text-center font-medium ${
                  checklist.allPassed
                    ? 'bg-green-500/20 text-green-500'
                    : 'bg-white/10 text-white/50'
                }`}
              >
                {checklist.allPassed
                  ? '✓ Ready for Prop Firm!'
                  : 'Keep trading to meet criteria'}
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function MetricCard({
  label,
  value,
  color = 'white',
}: {
  label: string;
  value: string;
  color?: 'green' | 'red' | 'yellow' | 'white';
}) {
  const colorClasses = {
    green: 'text-green-500',
    red: 'text-red-500',
    yellow: 'text-yellow-500',
    white: 'text-white',
  };

  return (
    <div className="card text-center py-3">
      <div className={`text-xl font-bold font-mono ${colorClasses[color]}`}>
        {value}
      </div>
      <div className="text-xs text-white/50 mt-1">{label}</div>
    </div>
  );
}

function CheckItem({
  label,
  current,
  passed,
  format = 'number',
}: {
  label: string;
  current: number;
  passed: boolean;
  format?: 'number' | 'percent' | 'decimal' | 'currency';
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

  return (
    <div className="flex items-center gap-3">
      <div
        className={`w-5 h-5 rounded-full flex items-center justify-center ${
          passed ? 'bg-green-500' : 'bg-white/20'
        }`}
      >
        {passed && <span className="text-xs">✓</span>}
      </div>
      <span className="flex-1 text-white/70">{label}</span>
      <span
        className={`font-mono ${passed ? 'text-green-500' : 'text-white/50'}`}
      >
        {formatValue(current)}
      </span>
    </div>
  );
}
