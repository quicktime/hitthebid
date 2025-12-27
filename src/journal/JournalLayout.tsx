import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';

export function JournalLayout() {
  const navigate = useNavigate();
  const { getTodaySession, getAllTimeStats, getOpenTrades } = useJournalStore();

  const todaySession = getTodaySession();
  const stats = getAllTimeStats();
  const openTrades = getOpenTrades();

  const navItems = [
    { path: '/journal', label: 'Dashboard', icon: '📊', end: true },
    { path: '/journal/trade', label: 'New Trade', icon: '➕', end: false },
    { path: '/journal/session', label: 'Session', icon: '📅', end: false },
    { path: '/journal/analytics', label: 'Analytics', icon: '📈', end: false },
    { path: '/journal/history', label: 'History', icon: '📜', end: false },
  ];

  return (
    <div className="min-h-screen bg-bg-primary flex flex-col">
      {/* Header */}
      <header className="bg-bg-secondary border-b border-border px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-6">
            {/* Logo */}
            <button
              onClick={() => navigate('/flow')}
              className="flex items-center gap-2 text-white/70 hover:text-white transition-colors"
            >
              <span className="text-xl">◉</span>
              <span className="font-mono font-bold tracking-wider">HITTHEBID</span>
            </button>

            {/* Navigation */}
            <nav className="flex items-center gap-1">
              {navItems.map((item) => (
                <NavLink
                  key={item.path}
                  to={item.path}
                  end={item.end}
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                      isActive
                        ? 'bg-white/10 text-white'
                        : 'text-white/60 hover:text-white hover:bg-white/5'
                    }`
                  }
                >
                  <span>{item.icon}</span>
                  <span>{item.label}</span>
                </NavLink>
              ))}
            </nav>
          </div>

          {/* Right side stats */}
          <div className="flex items-center gap-6">
            {/* Open trades indicator */}
            {openTrades.length > 0 && (
              <div className="flex items-center gap-2 px-3 py-1.5 bg-yellow-500/20 text-yellow-500 rounded-lg text-sm font-medium">
                <span className="relative flex h-2 w-2">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-yellow-400 opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-yellow-500"></span>
                </span>
                <span>{openTrades.length} Open</span>
              </div>
            )}

            {/* Quick stats */}
            <div className="flex items-center gap-4 text-sm">
              <div className="flex flex-col items-end">
                <span className="text-white/50 text-xs">Win Rate</span>
                <span
                  className={`font-mono font-bold ${
                    stats.winRate >= 50 ? 'text-green-500' : 'text-red-500'
                  }`}
                >
                  {stats.winRate.toFixed(1)}%
                </span>
              </div>
              <div className="flex flex-col items-end">
                <span className="text-white/50 text-xs">P&L</span>
                <span
                  className={`font-mono font-bold ${
                    stats.netPnl >= 0 ? 'text-green-500' : 'text-red-500'
                  }`}
                >
                  {stats.netPnl >= 0 ? '+' : ''}${stats.netPnl.toFixed(0)}
                </span>
              </div>
              <div className="flex flex-col items-end">
                <span className="text-white/50 text-xs">Trades</span>
                <span className="font-mono font-bold text-white">{stats.totalTrades}</span>
              </div>
            </div>

            {/* Today indicator */}
            <div
              className={`px-3 py-1.5 rounded-lg text-sm font-medium ${
                todaySession
                  ? 'bg-green-500/20 text-green-500'
                  : 'bg-white/10 text-white/60'
              }`}
            >
              {todaySession ? '✓ Session Active' : 'No Session'}
            </div>

            {/* Flow link */}
            <button
              onClick={() => navigate('/flow')}
              className="btn-ghost flex items-center gap-2"
            >
              <span>◉</span>
              <span>Flow</span>
            </button>
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className="flex-1 overflow-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}
