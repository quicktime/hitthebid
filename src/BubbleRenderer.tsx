import { useEffect, RefObject } from 'react';

interface Bubble {
  id: string;
  price: number;
  size: number;
  side: 'buy' | 'sell';
  timestamp: number;
  x: number;
  opacity: number;
}

interface BubbleRendererProps {
  bubbles: Bubble[];
  priceRange: { min: number; max: number } | null;
  canvasRef: RefObject<HTMLCanvasElement>;
}

// Colors matching trading aesthetic
const COLORS = {
  buy: {
    fill: 'rgba(0, 230, 118, 0.6)',
    stroke: 'rgba(0, 230, 118, 0.9)',
    glow: 'rgba(0, 230, 118, 0.3)'
  },
  sell: {
    fill: 'rgba(255, 82, 82, 0.6)',
    stroke: 'rgba(255, 82, 82, 0.9)',
    glow: 'rgba(255, 82, 82, 0.3)'
  },
  neutral: {
    fill: 'rgba(158, 158, 158, 0.5)',
    stroke: 'rgba(158, 158, 158, 0.8)'
  },
  grid: 'rgba(255, 255, 255, 0.05)',
  gridText: 'rgba(255, 255, 255, 0.4)',
  background: '#0a0a0a'
};

// Size scaling - adjust these for your preference
const MIN_BUBBLE_RADIUS = 4;
const MAX_BUBBLE_RADIUS = 60;
const SIZE_SCALE_FACTOR = 2; // Contracts per pixel radius

export function BubbleRenderer({ bubbles, priceRange, canvasRef }: BubbleRendererProps) {
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Set up high DPI canvas
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    // Clear canvas
    ctx.fillStyle = COLORS.background;
    ctx.fillRect(0, 0, rect.width, rect.height);

    if (!priceRange || bubbles.length === 0) {
      // Draw placeholder text
      ctx.fillStyle = 'rgba(255, 255, 255, 0.2)';
      ctx.font = '14px "JetBrains Mono", monospace';
      ctx.textAlign = 'center';
      ctx.fillText('Waiting for trades...', rect.width / 2, rect.height / 2);
      return;
    }

    const { min: priceMin, max: priceMax } = priceRange;
    const priceSpan = priceMax - priceMin;

    // Draw price grid
    drawPriceGrid(ctx, rect.width, rect.height, priceMin, priceMax);

    // Draw volume profile on right edge (optional enhancement)
    // drawVolumeProfile(ctx, bubbles, rect.width, rect.height, priceMin, priceMax);

    // Draw bubbles
    bubbles.forEach(bubble => {
      const x = bubble.x * rect.width;
      const y = rect.height - ((bubble.price - priceMin) / priceSpan) * rect.height;
      
      // Scale radius based on trade size
      const radius = Math.min(
        MAX_BUBBLE_RADIUS,
        Math.max(MIN_BUBBLE_RADIUS, Math.sqrt(bubble.size) * SIZE_SCALE_FACTOR)
      );

      const colors = bubble.side === 'buy' ? COLORS.buy : COLORS.sell;
      const opacity = bubble.opacity;

      // Draw glow effect for large trades
      if (bubble.size >= 10) {
        const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius * 2);
        gradient.addColorStop(0, colors.glow.replace('0.3', `${0.3 * opacity}`));
        gradient.addColorStop(1, 'transparent');
        ctx.fillStyle = gradient;
        ctx.beginPath();
        ctx.arc(x, y, radius * 2, 0, Math.PI * 2);
        ctx.fill();
      }

      // Draw main bubble
      ctx.globalAlpha = opacity;
      
      // Fill
      ctx.fillStyle = colors.fill;
      ctx.beginPath();
      ctx.arc(x, y, radius, 0, Math.PI * 2);
      ctx.fill();

      // Stroke
      ctx.strokeStyle = colors.stroke;
      ctx.lineWidth = 1.5;
      ctx.stroke();

      // Size label for large trades
      if (bubble.size >= 5 && radius > 15) {
        ctx.fillStyle = `rgba(255, 255, 255, ${0.9 * opacity})`;
        ctx.font = `bold ${Math.min(radius * 0.6, 14)}px "JetBrains Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(bubble.size.toString(), x, y);
      }

      ctx.globalAlpha = 1;
    });

    // Draw current price line
    const lastBubble = bubbles[bubbles.length - 1];
    if (lastBubble) {
      const lastY = rect.height - ((lastBubble.price - priceMin) / priceSpan) * rect.height;
      
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.3)';
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 4]);
      ctx.beginPath();
      ctx.moveTo(0, lastY);
      ctx.lineTo(rect.width, lastY);
      ctx.stroke();
      ctx.setLineDash([]);

      // Price label
      ctx.fillStyle = lastBubble.side === 'buy' ? COLORS.buy.stroke : COLORS.sell.stroke;
      ctx.font = 'bold 11px "JetBrains Mono", monospace';
      ctx.textAlign = 'right';
      ctx.fillText(lastBubble.price.toFixed(2), rect.width - 8, lastY - 8);
    }

  }, [bubbles, priceRange, canvasRef]);

  return (
    <canvas
      ref={canvasRef}
      className="bubble-canvas"
      style={{
        width: '100%',
        height: '100%',
        display: 'block'
      }}
    />
  );
}

function drawPriceGrid(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  priceMin: number,
  priceMax: number
) {
  const priceSpan = priceMax - priceMin;
  
  // Calculate nice price intervals
  const rawInterval = priceSpan / 8;
  const magnitude = Math.pow(10, Math.floor(Math.log10(rawInterval)));
  const normalized = rawInterval / magnitude;
  
  let interval: number;
  if (normalized < 1.5) interval = magnitude;
  else if (normalized < 3) interval = 2 * magnitude;
  else if (normalized < 7) interval = 5 * magnitude;
  else interval = 10 * magnitude;

  // Round to nice tick values
  const startPrice = Math.ceil(priceMin / interval) * interval;

  ctx.strokeStyle = COLORS.grid;
  ctx.lineWidth = 1;
  ctx.fillStyle = COLORS.gridText;
  ctx.font = '10px "JetBrains Mono", monospace';
  ctx.textAlign = 'right';

  for (let price = startPrice; price <= priceMax; price += interval) {
    const y = height - ((price - priceMin) / priceSpan) * height;
    
    // Grid line
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width - 50, y);
    ctx.stroke();

    // Price label
    ctx.fillText(price.toFixed(2), width - 8, y + 3);
  }
}

// Optional: Volume Profile on right side
export function drawVolumeProfile(
  ctx: CanvasRenderingContext2D,
  bubbles: Bubble[],
  width: number,
  height: number,
  priceMin: number,
  priceMax: number
) {
  const priceSpan = priceMax - priceMin;
  const bucketSize = priceSpan / 50; // 50 price buckets
  const volumeByPrice = new Map<number, { buy: number; sell: number }>();

  // Aggregate volume by price bucket
  bubbles.forEach(bubble => {
    const bucket = Math.floor((bubble.price - priceMin) / bucketSize);
    const existing = volumeByPrice.get(bucket) || { buy: 0, sell: 0 };
    if (bubble.side === 'buy') {
      existing.buy += bubble.size;
    } else {
      existing.sell += bubble.size;
    }
    volumeByPrice.set(bucket, existing);
  });

  // Find max volume for scaling
  let maxVol = 0;
  volumeByPrice.forEach(v => {
    maxVol = Math.max(maxVol, v.buy + v.sell);
  });

  if (maxVol === 0) return;

  const barHeight = height / 50;
  const maxBarWidth = 50;

  volumeByPrice.forEach((vol, bucket) => {
    const y = height - (bucket + 1) * barHeight;
    const totalWidth = ((vol.buy + vol.sell) / maxVol) * maxBarWidth;
    const buyWidth = (vol.buy / (vol.buy + vol.sell)) * totalWidth;
    const sellWidth = totalWidth - buyWidth;

    // Buy volume (green, left side of profile)
    ctx.fillStyle = COLORS.buy.fill;
    ctx.fillRect(width - 50 - buyWidth, y, buyWidth, barHeight - 1);

    // Sell volume (red, right side)
    ctx.fillStyle = COLORS.sell.fill;
    ctx.fillRect(width - 50, y, sellWidth, barHeight - 1);
  });
}
