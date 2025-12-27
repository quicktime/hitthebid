import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';
import { format } from 'date-fns';
import { Trade, TradingSession } from './types';

type ViewMode = 'trades' | 'sessions';
type SortField = 'date' | 'pnl' | 'rr' | 'grade';
type SortDir = 'asc' | 'desc';

export function TradeHistory() {
  const navigate = useNavigate();
  const { trades, sessions, exportData, importData, clearAllData } =
    useJournalStore();

  const [viewMode, setViewMode] = useState<ViewMode>('trades');
  const [sortField, setSortField] = useState<SortField>('date');
  const [sortDir, setSortDir] = useState<SortDir>('desc');
  const [filterGrade, setFilterGrade] = useState<string>('all');
  const [filterDirection, setFilterDirection] = useState<string>('all');
  const [showImportModal, setShowImportModal] = useState(false);
  const [importJson, setImportJson] = useState('');

  // Filter and sort trades
  const filteredTrades = trades
    .filter((t) => !t.isOpen)
    .filter((t) => filterGrade === 'all' || t.setupGrade === filterGrade)
    .filter((t) => filterDirection === 'all' || t.direction === filterDirection)
    .sort((a, b) => {
      let comparison = 0;
      switch (sortField) {
        case 'date':
          comparison =
            new Date(a.entryTime).getTime() - new Date(b.entryTime).getTime();
          break;
        case 'pnl':
          comparison = (a.pnl ?? 0) - (b.pnl ?? 0);
          break;
        case 'rr':
          comparison = (a.actualRR ?? 0) - (b.actualRR ?? 0);
          break;
        case 'grade':
          comparison = a.setupGrade.localeCompare(b.setupGrade);
          break;
      }
      return sortDir === 'asc' ? comparison : -comparison;
    });

  // Sort sessions
  const sortedSessions = [...sessions].sort(
    (a, b) => new Date(b.date).getTime() - new Date(a.date).getTime()
  );

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir(sortDir === 'asc' ? 'desc' : 'asc');
    } else {
      setSortField(field);
      setSortDir('desc');
    }
  };

  const handleExport = () => {
    const json = exportData();
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `hitthebid-journal-${format(new Date(), 'yyyy-MM-dd')}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };

  const handleImport = () => {
    if (importData(importJson)) {
      setShowImportModal(false);
      setImportJson('');
    } else {
      alert('Invalid JSON format');
    }
  };

  const handleClearData = () => {
    if (
      window.confirm(
        'Are you sure you want to delete ALL data? This cannot be undone.'
      )
    ) {
      clearAllData();
    }
  };

  const SortHeader = ({
    field,
    label,
  }: {
    field: SortField;
    label: string;
  }) => (
    <th
      className="cursor-pointer hover:bg-white/5 transition-colors"
      onClick={() => handleSort(field)}
    >
      <div className="flex items-center gap-1">
        {label}
        {sortField === field && (
          <span className="text-blue-500">{sortDir === 'asc' ? '↑' : '↓'}</span>
        )}
      </div>
    </th>
  );

  return (
    <div className="max-w-7xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-white">Trade History</h1>
        <div className="flex items-center gap-3">
          <button onClick={handleExport} className="btn-ghost">
            Export JSON
          </button>
          <button
            onClick={() => setShowImportModal(true)}
            className="btn-ghost"
          >
            Import
          </button>
          <button onClick={handleClearData} className="btn-danger">
            Clear All
          </button>
        </div>
      </div>

      {/* View Toggle and Filters */}
      <div className="card">
        <div className="flex items-center justify-between flex-wrap gap-4">
          <div className="flex items-center gap-2">
            <button
              className={`px-4 py-2 rounded-lg font-medium transition-all ${
                viewMode === 'trades'
                  ? 'bg-white/10 text-white'
                  : 'text-white/50 hover:text-white'
              }`}
              onClick={() => setViewMode('trades')}
            >
              Trades ({filteredTrades.length})
            </button>
            <button
              className={`px-4 py-2 rounded-lg font-medium transition-all ${
                viewMode === 'sessions'
                  ? 'bg-white/10 text-white'
                  : 'text-white/50 hover:text-white'
              }`}
              onClick={() => setViewMode('sessions')}
            >
              Sessions ({sessions.length})
            </button>
          </div>

          {viewMode === 'trades' && (
            <div className="flex items-center gap-4">
              <select
                className="select w-auto"
                value={filterGrade}
                onChange={(e) => setFilterGrade(e.target.value)}
              >
                <option value="all">All Grades</option>
                <option value="A">Grade A</option>
                <option value="B">Grade B</option>
                <option value="C">Grade C</option>
              </select>
              <select
                className="select w-auto"
                value={filterDirection}
                onChange={(e) => setFilterDirection(e.target.value)}
              >
                <option value="all">All Directions</option>
                <option value="long">Long Only</option>
                <option value="short">Short Only</option>
              </select>
            </div>
          )}
        </div>
      </div>

      {/* Content */}
      {viewMode === 'trades' ? (
        <div className="card overflow-x-auto">
          {filteredTrades.length === 0 ? (
            <div className="text-center py-12 text-white/50">
              No closed trades yet
            </div>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <SortHeader field="date" label="Date" />
                  <th>Direction</th>
                  <th>Location</th>
                  <th>Entry</th>
                  <th>Exit</th>
                  <SortHeader field="pnl" label="P&L" />
                  <SortHeader field="rr" label="R:R" />
                  <SortHeader field="grade" label="Grade" />
                  <th>Exit Type</th>
                </tr>
              </thead>
              <tbody>
                {filteredTrades.map((trade) => (
                  <TradeRow key={trade.id} trade={trade} />
                ))}
              </tbody>
            </table>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          {sortedSessions.length === 0 ? (
            <div className="card text-center py-12 text-white/50">
              No sessions yet
            </div>
          ) : (
            sortedSessions.map((session) => (
              <SessionRow
                key={session.id}
                session={session}
                onClick={() => navigate(`/journal/session?date=${session.date}`)}
              />
            ))
          )}
        </div>
      )}

      {/* Import Modal */}
      {showImportModal && (
        <div
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
          onClick={() => setShowImportModal(false)}
        >
          <div
            className="bg-bg-secondary border border-border rounded-lg p-6 w-full max-w-lg"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-lg font-bold text-white mb-4">Import Data</h3>
            <textarea
              className="input min-h-[200px] font-mono text-sm"
              value={importJson}
              onChange={(e) => setImportJson(e.target.value)}
              placeholder="Paste your JSON export here..."
            />
            <div className="flex gap-3 mt-4">
              <button
                onClick={() => setShowImportModal(false)}
                className="btn-ghost flex-1"
              >
                Cancel
              </button>
              <button onClick={handleImport} className="btn-primary flex-1">
                Import
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function TradeRow({ trade }: { trade: Trade }) {
  return (
    <tr>
      <td className="text-white/70">
        {format(new Date(trade.entryTime), 'MMM d, HH:mm')}
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
      <td className="text-white/70">
        {trade.locationType.toUpperCase().replace('_', ' ')}
      </td>
      <td className="font-mono">{trade.entryPrice.toFixed(2)}</td>
      <td className="font-mono">{trade.exitPrice?.toFixed(2) ?? '-'}</td>
      <td>
        <span
          className={`font-mono font-bold ${
            (trade.pnl ?? 0) >= 0 ? 'text-green-500' : 'text-red-500'
          }`}
        >
          {(trade.pnl ?? 0) >= 0 ? '+' : ''}${(trade.pnl ?? 0).toFixed(0)}
        </span>
      </td>
      <td>
        <span
          className={`font-mono ${
            (trade.actualRR ?? 0) >= 0 ? 'text-green-500' : 'text-red-500'
          }`}
        >
          {(trade.actualRR ?? 0).toFixed(2)}R
        </span>
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
      <td className="text-white/50 capitalize">
        {trade.exitType?.replace('_', ' ') ?? '-'}
      </td>
    </tr>
  );
}

function SessionRow({
  session,
  onClick,
}: {
  session: TradingSession;
  onClick: () => void;
}) {
  const winRate =
    session.totalTrades > 0
      ? ((session.winners / session.totalTrades) * 100).toFixed(1)
      : '0.0';

  return (
    <div
      className="card cursor-pointer hover:bg-bg-elevated transition-colors"
      onClick={onClick}
    >
      <div className="flex items-center justify-between">
        <div>
          <div className="text-lg font-medium text-white">
            {format(new Date(session.date), 'EEEE, MMMM d, yyyy')}
          </div>
          <div className="text-sm text-white/50 mt-1">
            {session.totalTrades} trades • {session.winners}W / {session.losers}L
            {session.scratches > 0 && ` / ${session.scratches}S`}
          </div>
        </div>
        <div className="flex items-center gap-6">
          <div className="text-right">
            <div className="text-xs text-white/50">Win Rate</div>
            <div
              className={`font-mono font-bold ${
                parseFloat(winRate) >= 50 ? 'text-green-500' : 'text-red-500'
              }`}
            >
              {winRate}%
            </div>
          </div>
          <div className="text-right">
            <div className="text-xs text-white/50">P&L</div>
            <div
              className={`font-mono font-bold text-xl ${
                session.netPnl >= 0 ? 'text-green-500' : 'text-red-500'
              }`}
            >
              {session.netPnl >= 0 ? '+' : ''}${session.netPnl.toFixed(0)}
            </div>
          </div>
          <div
            className={`px-3 py-1 rounded-lg text-sm font-medium ${
              session.premarketBias === 'bullish'
                ? 'bg-green-500/20 text-green-500'
                : session.premarketBias === 'bearish'
                ? 'bg-red-500/20 text-red-500'
                : 'bg-white/10 text-white/50'
            }`}
          >
            {session.premarketBias.charAt(0).toUpperCase() +
              session.premarketBias.slice(1)}
          </div>
        </div>
      </div>
      {session.dailyThesis && (
        <div className="mt-3 pt-3 border-t border-border">
          <p className="text-sm text-white/50 line-clamp-1">
            {session.dailyThesis}
          </p>
        </div>
      )}
    </div>
  );
}
