import { useState, useCallback } from 'react';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
  minSize: number;
  onMinSizeChange: (size: number) => void;
  isSoundEnabled: boolean;
  onSoundToggle: () => void;
  notificationsEnabled: boolean;
  notificationsPermission: NotificationPermission;
  onRequestNotificationPermission: () => void;
}

export function SettingsPanel({
  isOpen,
  onClose,
  minSize,
  onMinSizeChange,
  isSoundEnabled,
  onSoundToggle,
  notificationsEnabled,
  notificationsPermission,
  onRequestNotificationPermission,
}: SettingsPanelProps) {
  const [localMinSize, setLocalMinSize] = useState(minSize);

  const handleMinSizeChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseInt(e.target.value, 10);
      if (!isNaN(value) && value >= 1) {
        setLocalMinSize(value);
      }
    },
    []
  );

  const handleMinSizeApply = useCallback(() => {
    onMinSizeChange(localMinSize);
  }, [localMinSize, onMinSizeChange]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') {
        handleMinSizeApply();
      }
    },
    [handleMinSizeApply]
  );

  if (!isOpen) return null;

  return (
    <div className="settings-modal-overlay" onClick={onClose}>
      <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="settings-modal-header">
          <h3>Settings</h3>
          <button className="close-modal-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="settings-section">
          <h4>Trade Filters</h4>
          <div className="setting-item">
            <label htmlFor="min-size">Minimum Trade Size</label>
            <div className="setting-input-group">
              <input
                id="min-size"
                type="number"
                min="1"
                max="10000"
                value={localMinSize}
                onChange={handleMinSizeChange}
                onKeyDown={handleKeyDown}
                className="setting-input"
              />
              <button
                className="setting-apply-btn"
                onClick={handleMinSizeApply}
                disabled={localMinSize === minSize}
              >
                Apply
              </button>
            </div>
            <span className="setting-description">
              Only show trades with size &gt;= this value (current: {minSize})
            </span>
          </div>
        </div>

        <div className="settings-section">
          <h4>Alerts</h4>
          <div className="setting-item">
            <label>Sound Notifications</label>
            <div className="setting-toggle">
              <button
                className={`toggle-btn ${isSoundEnabled ? 'active' : ''}`}
                onClick={onSoundToggle}
              >
                {isSoundEnabled ? 'ON' : 'OFF'}
              </button>
            </div>
            <span className="setting-description">
              Play audio alerts for CVD flips, absorption events, and confluences
            </span>
          </div>

          <div className="setting-item">
            <label>Browser Notifications</label>
            <div className="setting-toggle">
              {notificationsPermission === 'granted' ? (
                <button
                  className={`toggle-btn ${notificationsEnabled ? 'active' : ''}`}
                  disabled
                >
                  ENABLED
                </button>
              ) : notificationsPermission === 'denied' ? (
                <button className="toggle-btn denied" disabled>
                  BLOCKED
                </button>
              ) : (
                <button
                  className="toggle-btn request"
                  onClick={onRequestNotificationPermission}
                >
                  Enable
                </button>
              )}
            </div>
            <span className="setting-description">
              {notificationsPermission === 'granted'
                ? 'Desktop notifications for confluence and strong signals (when tab not focused)'
                : notificationsPermission === 'denied'
                ? 'Notifications blocked - enable in browser settings'
                : 'Click to enable desktop notifications for important signals'}
            </span>
          </div>
        </div>

        <div className="settings-section">
          <h4>Signal Detection</h4>
          <div className="setting-item">
            <label>Absorption Sensitivity</label>
            <div className="sensitivity-indicator">
              <span className="sensitivity-label">Dynamic</span>
              <span className="sensitivity-info">Based on rolling volume average</span>
            </div>
            <span className="setting-description">
              Absorption detection uses dynamic thresholds based on market activity
            </span>
          </div>

          <div className="setting-item">
            <label>Stacked Imbalance Levels</label>
            <div className="sensitivity-indicator">
              <span className="sensitivity-label">3+ levels</span>
              <span className="sensitivity-info">70% imbalance ratio</span>
            </div>
            <span className="setting-description">
              Detects 3+ consecutive price levels with strong buy/sell imbalance
            </span>
          </div>

          <div className="setting-item">
            <label>Confluence Threshold</label>
            <div className="sensitivity-indicator">
              <span className="sensitivity-label">2+ signals</span>
              <span className="sensitivity-info">within 5 seconds</span>
            </div>
            <span className="setting-description">
              Triggers when multiple signals align in the same direction
            </span>
          </div>
        </div>

        <div className="settings-footer">
          <span className="settings-hint">
            Press Escape or click outside to close
          </span>
        </div>
      </div>
    </div>
  );
}
