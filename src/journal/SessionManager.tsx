import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useJournalStore } from '../stores/journalStore';
import { NewSessionForm, PremarketBias } from './types';
import { format } from 'date-fns';

export function SessionManager() {
  const navigate = useNavigate();
  const {
    getTodaySession,
    getSessionByDate,
    createSession,
    updateSession,
  } = useJournalStore();

  const today = format(new Date(), 'yyyy-MM-dd');
  const todaySession = getTodaySession();

  const [form, setForm] = useState<NewSessionForm>({
    date: today,
    pdh: '',
    pdl: '',
    pdc: '',
    onh: '',
    onl: '',
    poc: '',
    vah: '',
    val: '',
    lvnLevels: '',
    premarketBias: 'neutral',
    dailyThesis: '',
  });

  const [selectedDate, setSelectedDate] = useState(today);
  const [isEditing, setIsEditing] = useState(!todaySession);
  const [dailyReview, setDailyReview] = useState('');
  const [tomorrowFocus, setTomorrowFocus] = useState('');
  const [lessonsLearned, setLessonsLearned] = useState('');

  // Load session data when date changes
  useEffect(() => {
    const session = getSessionByDate(selectedDate);
    if (session) {
      setForm({
        date: session.date,
        pdh: session.pdh?.toString() ?? '',
        pdl: session.pdl?.toString() ?? '',
        pdc: session.pdc?.toString() ?? '',
        onh: session.onh?.toString() ?? '',
        onl: session.onl?.toString() ?? '',
        poc: session.poc?.toString() ?? '',
        vah: session.vah?.toString() ?? '',
        val: session.val?.toString() ?? '',
        lvnLevels: session.lvnLevels.join(', '),
        premarketBias: session.premarketBias,
        dailyThesis: session.dailyThesis,
      });
      setDailyReview(session.dailyReview);
      setTomorrowFocus(session.tomorrowFocus);
      setLessonsLearned(session.lessonsLearned);
      setIsEditing(false);
    } else {
      // Reset form for new session
      setForm({
        date: selectedDate,
        pdh: '',
        pdl: '',
        pdc: '',
        onh: '',
        onl: '',
        poc: '',
        vah: '',
        val: '',
        lvnLevels: '',
        premarketBias: 'neutral',
        dailyThesis: '',
      });
      setDailyReview('');
      setTomorrowFocus('');
      setLessonsLearned('');
      setIsEditing(true);
    }
  }, [selectedDate, getSessionByDate]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    const existingSession = getSessionByDate(form.date);

    if (existingSession) {
      // Update existing session
      updateSession(existingSession.id, {
        pdh: form.pdh ? parseFloat(form.pdh) : null,
        pdl: form.pdl ? parseFloat(form.pdl) : null,
        pdc: form.pdc ? parseFloat(form.pdc) : null,
        onh: form.onh ? parseFloat(form.onh) : null,
        onl: form.onl ? parseFloat(form.onl) : null,
        poc: form.poc ? parseFloat(form.poc) : null,
        vah: form.vah ? parseFloat(form.vah) : null,
        val: form.val ? parseFloat(form.val) : null,
        lvnLevels: form.lvnLevels
          .split(',')
          .map((s) => parseFloat(s.trim()))
          .filter((n) => !isNaN(n)),
        premarketBias: form.premarketBias,
        dailyThesis: form.dailyThesis,
        dailyReview,
        tomorrowFocus,
        lessonsLearned,
      });
    } else {
      // Create new session
      createSession(form);
    }

    setIsEditing(false);
    navigate('/journal');
  };

  const currentSession = getSessionByDate(selectedDate);

  return (
    <div className="max-w-4xl mx-auto">
      <div className="card">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-bold text-white">Session Manager</h2>
          <div className="flex items-center gap-3">
            <input
              type="date"
              className="input w-auto"
              value={selectedDate}
              onChange={(e) => setSelectedDate(e.target.value)}
            />
            {!isEditing && currentSession && (
              <button
                onClick={() => setIsEditing(true)}
                className="btn-ghost"
              >
                Edit
              </button>
            )}
          </div>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          {/* Pre-Market Levels */}
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider">
              Pre-Market Levels
            </h3>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div>
                <label className="label">PDH (Prior Day High)</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.pdh}
                  onChange={(e) => setForm({ ...form, pdh: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">PDL (Prior Day Low)</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.pdl}
                  onChange={(e) => setForm({ ...form, pdl: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">PDC (Prior Day Close)</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.pdc}
                  onChange={(e) => setForm({ ...form, pdc: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">POC</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.poc}
                  onChange={(e) => setForm({ ...form, poc: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div>
                <label className="label">ONH (Overnight High)</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.onh}
                  onChange={(e) => setForm({ ...form, onh: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">ONL (Overnight Low)</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.onl}
                  onChange={(e) => setForm({ ...form, onl: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">VAH</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.vah}
                  onChange={(e) => setForm({ ...form, vah: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
              <div>
                <label className="label">VAL</label>
                <input
                  type="number"
                  step="0.01"
                  className="input"
                  value={form.val}
                  onChange={(e) => setForm({ ...form, val: e.target.value })}
                  disabled={!isEditing}
                />
              </div>
            </div>

            <div>
              <label className="label">LVN Levels (comma-separated)</label>
              <input
                type="text"
                className="input"
                value={form.lvnLevels}
                onChange={(e) => setForm({ ...form, lvnLevels: e.target.value })}
                placeholder="21500.50, 21485.25, 21520.75"
                disabled={!isEditing}
              />
            </div>
          </div>

          {/* Daily Setup */}
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider">
              Daily Setup
            </h3>

            <div>
              <label className="label">Pre-Market Bias</label>
              <div className="flex gap-3">
                {(['bullish', 'neutral', 'bearish'] as PremarketBias[]).map(
                  (bias) => (
                    <button
                      key={bias}
                      type="button"
                      className={`flex-1 py-3 rounded-lg font-medium transition-all ${
                        form.premarketBias === bias
                          ? bias === 'bullish'
                            ? 'bg-green-500/20 text-green-500 border border-green-500'
                            : bias === 'bearish'
                            ? 'bg-red-500/20 text-red-500 border border-red-500'
                            : 'bg-white/20 text-white border border-white/50'
                          : 'bg-bg-tertiary text-white/50 border border-transparent'
                      } ${!isEditing ? 'opacity-50 cursor-not-allowed' : ''}`}
                      onClick={() =>
                        isEditing && setForm({ ...form, premarketBias: bias })
                      }
                      disabled={!isEditing}
                    >
                      {bias === 'bullish' && '↗ '}
                      {bias === 'bearish' && '↘ '}
                      {bias.charAt(0).toUpperCase() + bias.slice(1)}
                    </button>
                  )
                )}
              </div>
            </div>

            <div>
              <label className="label">Daily Thesis</label>
              <textarea
                className="input min-h-[100px]"
                value={form.dailyThesis}
                onChange={(e) =>
                  setForm({ ...form, dailyThesis: e.target.value })
                }
                placeholder="What's your plan for today? What setups are you looking for?"
                disabled={!isEditing}
              />
            </div>
          </div>

          {/* Post-Session Review (only show if session exists) */}
          {currentSession && (
            <div className="space-y-4">
              <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider">
                Post-Session Review
              </h3>

              <div>
                <label className="label">Daily Review</label>
                <textarea
                  className="input min-h-[100px]"
                  value={dailyReview}
                  onChange={(e) => setDailyReview(e.target.value)}
                  placeholder="How did the session go? What happened?"
                  disabled={!isEditing}
                />
              </div>

              <div>
                <label className="label">Lessons Learned</label>
                <textarea
                  className="input min-h-[80px]"
                  value={lessonsLearned}
                  onChange={(e) => setLessonsLearned(e.target.value)}
                  placeholder="What did you learn today?"
                  disabled={!isEditing}
                />
              </div>

              <div>
                <label className="label">Tomorrow's Focus</label>
                <textarea
                  className="input min-h-[80px]"
                  value={tomorrowFocus}
                  onChange={(e) => setTomorrowFocus(e.target.value)}
                  placeholder="What will you focus on tomorrow?"
                  disabled={!isEditing}
                />
              </div>
            </div>
          )}

          {/* Session Stats (if exists) */}
          {currentSession && (
            <div className="p-4 bg-bg-tertiary rounded-lg">
              <h3 className="text-sm font-medium text-white/50 uppercase tracking-wider mb-4">
                Session Statistics
              </h3>
              <div className="grid grid-cols-2 md:grid-cols-5 gap-4 text-center">
                <div>
                  <div className="text-2xl font-bold font-mono text-white">
                    {currentSession.totalTrades}
                  </div>
                  <div className="text-xs text-white/50">Total Trades</div>
                </div>
                <div>
                  <div className="text-2xl font-bold font-mono text-green-500">
                    {currentSession.winners}
                  </div>
                  <div className="text-xs text-white/50">Winners</div>
                </div>
                <div>
                  <div className="text-2xl font-bold font-mono text-red-500">
                    {currentSession.losers}
                  </div>
                  <div className="text-xs text-white/50">Losers</div>
                </div>
                <div>
                  <div
                    className={`text-2xl font-bold font-mono ${
                      currentSession.netPnl >= 0
                        ? 'text-green-500'
                        : 'text-red-500'
                    }`}
                  >
                    {currentSession.netPnl >= 0 ? '+' : ''}$
                    {currentSession.netPnl.toFixed(0)}
                  </div>
                  <div className="text-xs text-white/50">Net P&L</div>
                </div>
                <div>
                  <div className="text-2xl font-bold font-mono text-yellow-500">
                    ${currentSession.maxDrawdownFromHigh.toFixed(0)}
                  </div>
                  <div className="text-xs text-white/50">Max DD</div>
                </div>
              </div>
            </div>
          )}

          {/* Action buttons */}
          {isEditing && (
            <div className="flex gap-3 pt-4">
              <button
                type="button"
                onClick={() => {
                  if (currentSession) {
                    setIsEditing(false);
                  } else {
                    navigate('/journal');
                  }
                }}
                className="btn-ghost flex-1"
              >
                Cancel
              </button>
              <button type="submit" className="btn-primary flex-1">
                {currentSession ? 'Update Session' : 'Start Session'}
              </button>
            </div>
          )}
        </form>
      </div>
    </div>
  );
}
