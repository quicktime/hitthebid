// Demo mode data generator for testing the visualization
// Generates realistic-looking trade data without requiring a live connection

import { Trade } from './tradovate';

export class DemoDataGenerator {
  private isRunning = false;
  private intervalId: NodeJS.Timeout | null = null;
  private currentPrice: number;
  private symbol: string;
  private callbacks: ((trade: Trade) => void)[] = [];
  
  // NQ trades around 21000, ES around 5900 - adjust as market moves
  private readonly basePrice = {
    'NQH5': 21800,
    'NQM5': 21800,
    'ESH5': 5950,
    'ESM5': 5950
  };

  constructor(symbol: string = 'NQH5') {
    this.symbol = symbol;
    this.currentPrice = this.basePrice[symbol as keyof typeof this.basePrice] || 21800;
  }

  start(): void {
    if (this.isRunning) return;
    
    this.isRunning = true;
    console.log('🎮 Demo mode started');

    // Generate trades at varying intervals (50-500ms)
    const generateTrade = () => {
      if (!this.isRunning) return;

      const trade = this.generateRandomTrade();
      this.callbacks.forEach(cb => cb(trade));

      // Random interval for next trade (more realistic than fixed interval)
      const nextInterval = 50 + Math.random() * 450;
      this.intervalId = setTimeout(generateTrade, nextInterval);
    };

    generateTrade();
  }

  stop(): void {
    this.isRunning = false;
    if (this.intervalId) {
      clearTimeout(this.intervalId);
      this.intervalId = null;
    }
    console.log('🎮 Demo mode stopped');
  }

  setSymbol(symbol: string): void {
    this.symbol = symbol;
    this.currentPrice = this.basePrice[symbol as keyof typeof this.basePrice] || this.currentPrice;
  }

  onTrade(callback: (trade: Trade) => void): void {
    this.callbacks.push(callback);
  }

  private generateRandomTrade(): Trade {
    // Random walk for price
    const priceChange = (Math.random() - 0.5) * 2; // -1 to +1 points
    this.currentPrice += priceChange;
    
    // Round to tick size (0.25 for NQ/ES)
    this.currentPrice = Math.round(this.currentPrice * 4) / 4;

    // Size distribution - heavily weighted toward large trades for visual impact
    // Creates dramatic overlapping bubbles
    const sizeRandom = Math.random();
    let size: number;

    if (sizeRandom < 0.2) {
      // 20% chance: 10-30 contracts (medium)
      size = 10 + Math.floor(Math.random() * 21);
    } else if (sizeRandom < 0.5) {
      // 30% chance: 31-75 contracts (large)
      size = 31 + Math.floor(Math.random() * 45);
    } else if (sizeRandom < 0.8) {
      // 30% chance: 76-150 contracts (very large)
      size = 76 + Math.floor(Math.random() * 75);
    } else {
      // 20% chance: 151-300 contracts (institutional)
      size = 151 + Math.floor(Math.random() * 150);
    }

    // Slight bias based on price direction for realism
    // If price went up, slightly more likely to be buy aggression
    const buyProbability = 0.5 + (priceChange > 0 ? 0.1 : -0.1);
    const side = Math.random() < buyProbability ? 'buy' : 'sell';

    return {
      symbol: this.symbol,
      price: this.currentPrice,
      size,
      side,
      timestamp: Date.now()
    };
  }
}

// Singleton for easy access
let demoGenerator: DemoDataGenerator | null = null;

export function getDemoGenerator(symbol?: string): DemoDataGenerator {
  if (!demoGenerator) {
    demoGenerator = new DemoDataGenerator(symbol);
  }
  return demoGenerator;
}
