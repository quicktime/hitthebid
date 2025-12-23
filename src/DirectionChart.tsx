import { useEffect, useRef } from 'react';

interface DirectionChartProps {
  bullish: number;
  bearish: number;
  title: string;
}

export function DirectionChart({ bullish, bearish, title }: DirectionChartProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Get device pixel ratio for crisp rendering
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();

    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const width = rect.width;
    const height = rect.height;
    const centerX = width / 2;
    const centerY = height / 2;
    const radius = Math.min(width, height) / 2 - 20;

    // Clear canvas
    ctx.clearRect(0, 0, width, height);

    const total = bullish + bearish;
    if (total === 0) {
      // Draw empty state
      ctx.beginPath();
      ctx.arc(centerX, centerY, radius, 0, Math.PI * 2);
      ctx.fillStyle = '#1a1a1a';
      ctx.fill();
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.2)';
      ctx.lineWidth = 2;
      ctx.stroke();

      ctx.fillStyle = 'rgba(255, 255, 255, 0.4)';
      ctx.font = '12px JetBrains Mono, monospace';
      ctx.textAlign = 'center';
      ctx.fillText('No data', centerX, centerY + 4);
      return;
    }

    const bullishAngle = (bullish / total) * Math.PI * 2;
    const bearishAngle = (bearish / total) * Math.PI * 2;

    // Draw bullish slice (green)
    ctx.beginPath();
    ctx.moveTo(centerX, centerY);
    ctx.arc(centerX, centerY, radius, -Math.PI / 2, -Math.PI / 2 + bullishAngle);
    ctx.closePath();
    ctx.fillStyle = '#00e676';
    ctx.fill();

    // Draw bearish slice (red)
    ctx.beginPath();
    ctx.moveTo(centerX, centerY);
    ctx.arc(centerX, centerY, radius, -Math.PI / 2 + bullishAngle, -Math.PI / 2 + bullishAngle + bearishAngle);
    ctx.closePath();
    ctx.fillStyle = '#ff5252';
    ctx.fill();

    // Draw center hole (donut effect)
    ctx.beginPath();
    ctx.arc(centerX, centerY, radius * 0.55, 0, Math.PI * 2);
    ctx.fillStyle = '#111111';
    ctx.fill();

    // Draw total in center
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 18px JetBrains Mono, monospace';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(total.toString(), centerX, centerY - 6);

    ctx.fillStyle = 'rgba(255, 255, 255, 0.5)';
    ctx.font = '10px JetBrains Mono, monospace';
    ctx.fillText('TOTAL', centerX, centerY + 10);

  }, [bullish, bearish]);

  const total = bullish + bearish;
  const bullishPct = total > 0 ? ((bullish / total) * 100).toFixed(0) : '0';
  const bearishPct = total > 0 ? ((bearish / total) * 100).toFixed(0) : '0';

  return (
    <div className="direction-chart">
      <h4 className="chart-title">{title}</h4>
      <canvas ref={canvasRef} className="pie-canvas" />
      <div className="chart-legend">
        <div className="legend-item bullish">
          <span className="legend-color"></span>
          <span className="legend-label">Bullish</span>
          <span className="legend-value">{bullish} ({bullishPct}%)</span>
        </div>
        <div className="legend-item bearish">
          <span className="legend-color"></span>
          <span className="legend-label">Bearish</span>
          <span className="legend-value">{bearish} ({bearishPct}%)</span>
        </div>
      </div>
    </div>
  );
}
