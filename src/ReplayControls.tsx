import { useCallback } from 'react';

interface ReplayStatus {
  mode: string;
  isPaused: boolean;
  speed: number;
  replayDate: string | null;
  replayProgress: number | null;
  currentTime: number | null;
}

interface ReplayControlsProps {
  status: ReplayStatus;
  onPause: () => void;
  onResume: () => void;
  onSpeedChange: (speed: number) => void;
}

export function ReplayControls({ status, onPause, onResume, onSpeedChange }: ReplayControlsProps) {
  const handleSpeedChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const speed = parseInt(e.target.value, 10);
      onSpeedChange(speed);
    },
    [onSpeedChange]
  );

  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString('en-US', {
      hour: 'numeric',
      minute: '2-digit',
      second: '2-digit',
      hour12: true,
    });
  };

  const speedPresets = [1, 2, 5, 10, 20, 50];

  return (
    <div className="replay-controls">
      <div className="replay-header">
        <span className="replay-badge">REPLAY</span>
        {status.replayDate && <span className="replay-date">{status.replayDate}</span>}
      </div>

      <div className="replay-playback">
        <button
          className={`playback-btn ${status.isPaused ? 'paused' : 'playing'}`}
          onClick={status.isPaused ? onResume : onPause}
          title={status.isPaused ? 'Resume (Space)' : 'Pause (Space)'}
        >
          {status.isPaused ? '▶' : '⏸'}
        </button>

        {status.currentTime && (
          <span className="replay-time">{formatTime(status.currentTime)}</span>
        )}
      </div>

      <div className="replay-speed">
        <label>Speed</label>
        <input
          type="range"
          min="1"
          max="50"
          value={status.speed}
          onChange={handleSpeedChange}
          className="speed-slider"
        />
        <span className="speed-value">{status.speed}x</span>
      </div>

      <div className="speed-presets">
        {speedPresets.map((preset) => (
          <button
            key={preset}
            className={`preset-btn ${status.speed === preset ? 'active' : ''}`}
            onClick={() => onSpeedChange(preset)}
          >
            {preset}x
          </button>
        ))}
      </div>
    </div>
  );
}
