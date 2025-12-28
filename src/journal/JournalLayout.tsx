import { NavLink, Outlet, Link } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';
import '../App.css';

export function JournalLayout() {
  const { getTodaySession, getAllTimeStats, getOpenTrades } = useJournalStore();

  const todaySession = getTodaySession();
  const stats = getAllTimeStats();
  const openTrades = getOpenTrades();

  const navItems = [
    { path: '/flow', label: 'Flow', end: false },
    { path: '/journal', label: 'Dashboard', end: true },
    { path: '/journal/trade', label: 'Trade', end: false },
    { path: '/journal/session', label: 'Session', end: false },
    { path: '/journal/analytics', label: 'Analytics', end: false },
    { path: '/journal/history', label: 'History', end: false },
  ];

  return (
    <div className="app">
      {/* Header - matches Flow exactly */}
      <header className="header">
        <div className="header-left">
          <h1 className="logo">
            <span className="logo-icon">◉</span>
            HIT
          </h1>

          {/* Main Navigation - same style as Flow */}
          <div className="symbol-selector">
            {navItems.map((item) => (
              <NavLink
                key={item.path}
                to={item.path}
                end={item.end}
                className={({ isActive }) => `symbol-btn ${isActive ? 'active' : ''}`}
              >
                {item.label}
              </NavLink>
            ))}
          </div>
        </div>

        <div className="header-center">
          {/* Empty to match Flow */}
        </div>

        <div className="header-right">
          {/* Open trades indicator */}
          {openTrades.length > 0 && (
            <div className="status demo">
              <span className="status-dot"></span>
              {openTrades.length} Open
            </div>
          )}

          {/* Session status - matches Flow's OFFLINE style */}
          <div className={`status ${todaySession ? 'connected' : ''}`}>
            <span className="status-dot"></span>
            {todaySession ? 'ACTIVE' : 'NO SESSION'}
          </div>
        </div>
      </header>

      {/* Main content */}
      <main style={{ flex: 1, overflow: 'auto', padding: '20px', background: 'var(--bg-primary)' }}>
        <Outlet />
      </main>
    </div>
  );
}
