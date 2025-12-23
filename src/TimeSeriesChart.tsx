import { useRef, useEffect } from 'react';

interface Signal {
  timestamp: number;
  outcome: string | null;
  direction: string;
}

interface TimeSeriesChartProps {
  signals: Signal[];
  title?: string;
}

const CHART_COLORS = {
  green: '#00e676',
  red: '#ff5252',
  yellow: '#ffc107',
  blue: '#448aff',
  textPrimary: '#ffffff',
  textSecondary: 'rgba(255, 255, 255, 0.7)',
  textMuted: 'rgba(255, 255, 255, 0.4)',
  bgTertiary: '#1a1a1a',
  border: 'rgba(255, 255, 255, 0.08)',
};

interface TimeBucket {
  label: string;
  startTime: number;
  wins: number;
  losses: number;
  total: number;
  winRate: number;
}

export function TimeSeriesChart({ signals, title = 'Win Rate Over Time' }: TimeSeriesChartProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Handle high DPI displays
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    // Clear canvas
    ctx.clearRect(0, 0, rect.width, rect.height);

    // Filter signals with outcomes
    const signalsWithOutcome = signals.filter(s => s.outcome === 'win' || s.outcome === 'loss');

    if (signalsWithOutcome.length < 2) {
      ctx.font = '14px "Space Grotesk", sans-serif';
      ctx.fillStyle = CHART_COLORS.textMuted;
      ctx.textAlign = 'center';
      ctx.fillText('Need more signals with outcomes', rect.width / 2, rect.height / 2);
      return;
    }

    // Create time buckets (hourly)
    const buckets = createTimeBuckets(signalsWithOutcome);
    if (buckets.length === 0) return;

    drawTimeSeriesChart(ctx, buckets, rect.width, rect.height);
  }, [signals]);

  return (
    <div className="time-series-chart">
      <h3>{title}</h3>
      <canvas ref={canvasRef} className="time-series-canvas" />
    </div>
  );
}

function createTimeBuckets(signals: Signal[]): TimeBucket[] {
  if (signals.length === 0) return [];

  // Sort by timestamp
  const sorted = [...signals].sort((a, b) => a.timestamp - b.timestamp);

  // Determine bucket size based on data range
  const minTime = sorted[0].timestamp;
  const maxTime = sorted[sorted.length - 1].timestamp;
  const timeRange = maxTime - minTime;

  // Use hourly buckets for ranges under 24 hours, otherwise daily
  const hourMs = 60 * 60 * 1000;
  const dayMs = 24 * hourMs;
  const bucketSize = timeRange > dayMs * 3 ? dayMs : hourMs;

  // Create buckets
  const bucketMap = new Map<number, TimeBucket>();

  sorted.forEach(signal => {
    const bucketStart = Math.floor(signal.timestamp / bucketSize) * bucketSize;

    if (!bucketMap.has(bucketStart)) {
      const date = new Date(bucketStart);
      const label = bucketSize === dayMs
        ? date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
        : date.toLocaleTimeString('en-US', { hour: 'numeric', hour12: true });

      bucketMap.set(bucketStart, {
        label,
        startTime: bucketStart,
        wins: 0,
        losses: 0,
        total: 0,
        winRate: 0,
      });
    }

    const bucket = bucketMap.get(bucketStart)!;
    bucket.total++;
    if (signal.outcome === 'win') {
      bucket.wins++;
    } else if (signal.outcome === 'loss') {
      bucket.losses++;
    }
  });

  // Calculate win rates
  const buckets = Array.from(bucketMap.values())
    .sort((a, b) => a.startTime - b.startTime)
    .slice(-12); // Keep last 12 buckets

  buckets.forEach(bucket => {
    bucket.winRate = bucket.total > 0 ? (bucket.wins / bucket.total) * 100 : 0;
  });

  return buckets;
}

function drawTimeSeriesChart(
  ctx: CanvasRenderingContext2D,
  buckets: TimeBucket[],
  width: number,
  height: number
) {
  const padding = { top: 30, right: 20, bottom: 50, left: 50 };
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;

  // Draw Y-axis
  ctx.strokeStyle = CHART_COLORS.border;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(padding.left, padding.top);
  ctx.lineTo(padding.left, padding.top + chartHeight);
  ctx.stroke();

  // Draw X-axis
  ctx.beginPath();
  ctx.moveTo(padding.left, padding.top + chartHeight);
  ctx.lineTo(padding.left + chartWidth, padding.top + chartHeight);
  ctx.stroke();

  // Draw Y-axis labels
  ctx.font = '10px "JetBrains Mono", monospace';
  ctx.fillStyle = CHART_COLORS.textMuted;
  ctx.textAlign = 'right';

  const yLabels = [0, 25, 50, 75, 100];
  yLabels.forEach(percent => {
    const y = padding.top + chartHeight - (chartHeight * percent / 100);
    ctx.fillText(`${percent}%`, padding.left - 8, y + 4);

    ctx.strokeStyle = CHART_COLORS.border;
    ctx.beginPath();
    ctx.moveTo(padding.left, y);
    ctx.lineTo(padding.left + chartWidth, y);
    ctx.stroke();
  });

  // Draw 50% reference line
  ctx.strokeStyle = CHART_COLORS.yellow;
  ctx.setLineDash([5, 5]);
  const fiftyPercentY = padding.top + chartHeight / 2;
  ctx.beginPath();
  ctx.moveTo(padding.left, fiftyPercentY);
  ctx.lineTo(padding.left + chartWidth, fiftyPercentY);
  ctx.stroke();
  ctx.setLineDash([]);

  if (buckets.length === 0) return;

  // Calculate positions
  const xStep = chartWidth / Math.max(buckets.length - 1, 1);

  // Draw line chart
  ctx.strokeStyle = CHART_COLORS.blue;
  ctx.lineWidth = 2;
  ctx.beginPath();

  const points: Array<{ x: number; y: number; bucket: TimeBucket }> = [];

  buckets.forEach((bucket, i) => {
    const x = padding.left + (i * xStep);
    const y = padding.top + chartHeight - (chartHeight * bucket.winRate / 100);
    points.push({ x, y, bucket });

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  });

  ctx.stroke();

  // Draw area fill
  ctx.beginPath();
  ctx.moveTo(points[0].x, padding.top + chartHeight);
  points.forEach(point => ctx.lineTo(point.x, point.y));
  ctx.lineTo(points[points.length - 1].x, padding.top + chartHeight);
  ctx.closePath();

  const gradient = ctx.createLinearGradient(0, padding.top, 0, padding.top + chartHeight);
  gradient.addColorStop(0, 'rgba(68, 138, 255, 0.3)');
  gradient.addColorStop(1, 'rgba(68, 138, 255, 0.05)');
  ctx.fillStyle = gradient;
  ctx.fill();

  // Draw data points
  points.forEach((point, i) => {
    const { x, y, bucket } = point;

    // Point color based on win rate
    let color: string;
    if (bucket.winRate >= 60) {
      color = CHART_COLORS.green;
    } else if (bucket.winRate >= 50) {
      color = CHART_COLORS.yellow;
    } else {
      color = CHART_COLORS.red;
    }

    // Draw point
    ctx.beginPath();
    ctx.arc(x, y, 5, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.strokeStyle = '#111';
    ctx.lineWidth = 2;
    ctx.stroke();

    // Draw X-axis labels (every 2nd for readability)
    if (i % 2 === 0 || buckets.length <= 6) {
      ctx.font = '9px "JetBrains Mono", monospace';
      ctx.fillStyle = CHART_COLORS.textMuted;
      ctx.textAlign = 'center';
      ctx.fillText(bucket.label, x, padding.top + chartHeight + 16);
    }

    // Draw win rate label above point
    ctx.font = 'bold 10px "JetBrains Mono", monospace';
    ctx.fillStyle = color;
    ctx.textAlign = 'center';
    ctx.fillText(`${bucket.winRate.toFixed(0)}%`, x, y - 12);
  });
}
