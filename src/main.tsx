import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import App from './App'
import { JournalLayout } from './journal/JournalLayout'
import { JournalDashboard } from './journal/JournalDashboard'
import { TradeEntry } from './journal/TradeEntry'
import { SessionManager } from './journal/SessionManager'
import { AnalyticsDashboard } from './journal/AnalyticsDashboard'
import { TradeHistory } from './journal/TradeHistory'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        {/* Flow visualization (existing app) */}
        <Route path="/flow" element={<App />} />

        {/* Journal routes */}
        <Route path="/journal" element={<JournalLayout />}>
          <Route index element={<JournalDashboard />} />
          <Route path="trade" element={<TradeEntry />} />
          <Route path="session" element={<SessionManager />} />
          <Route path="analytics" element={<AnalyticsDashboard />} />
          <Route path="history" element={<TradeHistory />} />
        </Route>

        {/* Default redirect to flow */}
        <Route path="/" element={<Navigate to="/flow" replace />} />
      </Routes>
    </BrowserRouter>
  </React.StrictMode>,
)
