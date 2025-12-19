// Tradovate API Connection Handler
// Docs: https://api.tradovate.com/

export interface Trade {
  symbol: string;
  price: number;
  size: number;
  side: 'buy' | 'sell';
  timestamp: number;
}

export interface TradovateCredentials {
  username: string;
  password: string;
  appId: string;
  appVersion: string;
  cid: string;  // Client ID
  sec: string;  // Secret
}

interface Quote {
  bid: number;
  ask: number;
  bidSize: number;
  askSize: number;
}

// Tradovate uses demo and live environments
const DEMO_API_URL = 'https://demo.tradovateapi.com/v1';
const LIVE_API_URL = 'https://live.tradovateapi.com/v1';
const DEMO_WS_URL = 'wss://demo.tradovateapi.com/v1/websocket';
const LIVE_WS_URL = 'wss://live.tradovateapi.com/v1/websocket';
const MD_WS_URL = 'wss://md.tradovateapi.com/v1/websocket'; // Market data WebSocket

export class TradovateConnection {
  private credentials: TradovateCredentials;
  private accessToken: string | null = null;
  private mdSocket: WebSocket | null = null;
  private tradeCallbacks: ((trade: Trade) => void)[] = [];
  private disconnectCallbacks: (() => void)[] = [];
  private quotes: Map<string, Quote> = new Map();
  private contractIds: Map<string, number> = new Map();
  private requestId = 1;
  private pendingRequests: Map<number, { resolve: (data: any) => void; reject: (err: Error) => void }> = new Map();
  private isDemo = true; // Set to false for live trading

  constructor(credentials: TradovateCredentials, isDemo = true) {
    this.credentials = credentials;
    this.isDemo = isDemo;
  }

  private get apiUrl(): string {
    return this.isDemo ? DEMO_API_URL : LIVE_API_URL;
  }

  async connect(): Promise<void> {
    // Step 1: Authenticate and get access token
    await this.authenticate();
    
    // Step 2: Connect to market data WebSocket
    await this.connectMarketDataSocket();
  }

  private async authenticate(): Promise<void> {
    const authResponse = await fetch(`${this.apiUrl}/auth/accesstokenrequest`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json'
      },
      body: JSON.stringify({
        name: this.credentials.username,
        password: this.credentials.password,
        appId: this.credentials.appId,
        appVersion: this.credentials.appVersion,
        cid: this.credentials.cid,
        sec: this.credentials.sec
      })
    });

    if (!authResponse.ok) {
      const errorText = await authResponse.text();
      throw new Error(`Authentication failed: ${authResponse.status} - ${errorText}`);
    }

    const authData = await authResponse.json();
    
    if (authData.errorText) {
      throw new Error(`Authentication error: ${authData.errorText}`);
    }

    this.accessToken = authData.accessToken;
    
    if (!this.accessToken) {
      throw new Error('No access token received');
    }

    console.log('✓ Authenticated with Tradovate');
  }

  private async connectMarketDataSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.mdSocket = new WebSocket(MD_WS_URL);

      const timeout = setTimeout(() => {
        reject(new Error('WebSocket connection timeout'));
      }, 10000);

      this.mdSocket.onopen = () => {
        console.log('✓ Market data WebSocket connected');
        
        // Authorize the WebSocket connection
        this.sendMdRequest('authorize', this.accessToken)
          .then(() => {
            clearTimeout(timeout);
            console.log('✓ WebSocket authorized');
            resolve();
          })
          .catch(reject);
      };

      this.mdSocket.onmessage = (event) => {
        this.handleMdMessage(event.data);
      };

      this.mdSocket.onerror = (error) => {
        console.error('WebSocket error:', error);
        clearTimeout(timeout);
        reject(new Error('WebSocket connection error'));
      };

      this.mdSocket.onclose = () => {
        console.log('WebSocket closed');
        this.disconnectCallbacks.forEach(cb => cb());
      };
    });
  }

  private handleMdMessage(data: string): void {
    // Tradovate sends messages in a specific format
    // First character indicates message type: 'a' for array, 'o' for open, 'h' for heartbeat
    
    if (data === 'o') {
      // Connection opened
      return;
    }

    if (data === 'h') {
      // Heartbeat - respond with heartbeat
      this.mdSocket?.send('[]');
      return;
    }

    if (data.startsWith('a')) {
      try {
        const messages = JSON.parse(data.slice(1));
        messages.forEach((msg: any) => this.processMdMessage(msg));
      } catch (e) {
        console.error('Failed to parse message:', e);
      }
    }
  }

  private processMdMessage(msg: any): void {
    // Handle different message types
    if (msg.e === 'props') {
      // Response to a request
      const id = msg.i;
      const pending = this.pendingRequests.get(id);
      if (pending) {
        this.pendingRequests.delete(id);
        if (msg.s === 200) {
          pending.resolve(msg.d);
        } else {
          pending.reject(new Error(msg.d?.errorText || 'Request failed'));
        }
      }
    }

    // Quote updates
    if (msg.e === 'md' && msg.d) {
      const { contractId } = msg.d;
      const symbol = this.getSymbolForContractId(contractId);
      
      if (msg.d.quotes) {
        // Quote data
        const quote = msg.d.quotes;
        this.quotes.set(symbol, {
          bid: quote.bid?.price || 0,
          ask: quote.ask?.price || 0,
          bidSize: quote.bid?.size || 0,
          askSize: quote.ask?.size || 0
        });
      }

      if (msg.d.trades) {
        // Trade data - this is what we want!
        msg.d.trades.forEach((trade: any) => {
          this.processTrade(symbol, trade);
        });
      }

      // DOM updates also contain trade info
      if (msg.d.dom) {
        // Process DOM data if needed
      }
    }

    // Chart/histogram data
    if (msg.e === 'chart') {
      // Historical chart data
    }
  }

  private processTrade(symbol: string, tradeData: any): void {
    const price = tradeData.price;
    const size = tradeData.size || tradeData.volume || 1;
    const quote = this.quotes.get(symbol);
    
    // Determine buy/sell based on trade price vs bid/ask
    // If trade is at or above ask -> buy aggression
    // If trade is at or below bid -> sell aggression
    let side: 'buy' | 'sell' = 'buy';
    
    if (quote) {
      if (price <= quote.bid) {
        side = 'sell';
      } else if (price >= quote.ask) {
        side = 'buy';
      } else {
        // Between bid and ask - use previous trade direction or default
        side = tradeData.aggressor === 'Buy' ? 'buy' : 'sell';
      }
    } else if (tradeData.aggressor) {
      side = tradeData.aggressor === 'Buy' ? 'buy' : 'sell';
    }

    const trade: Trade = {
      symbol,
      price,
      size,
      side,
      timestamp: tradeData.timestamp || Date.now()
    };

    this.tradeCallbacks.forEach(cb => cb(trade));
  }

  private getSymbolForContractId(contractId: number): string {
    for (const [symbol, id] of this.contractIds) {
      if (id === contractId) return symbol;
    }
    return 'UNKNOWN';
  }

  private async sendMdRequest(url: string, body?: any): Promise<any> {
    return new Promise((resolve, reject) => {
      const id = this.requestId++;
      
      this.pendingRequests.set(id, { resolve, reject });

      const message = JSON.stringify({
        i: id,
        url,
        body
      });

      // Tradovate expects array-wrapped messages
      this.mdSocket?.send(`[${message}]`);

      // Timeout after 10 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`Request timeout for ${url}`));
        }
      }, 10000);
    });
  }

  async subscribeToSymbol(symbol: string): Promise<void> {
    if (!this.mdSocket || this.mdSocket.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }

    // First, get the contract ID for the symbol
    const contractId = await this.getContractId(symbol);
    this.contractIds.set(symbol, contractId);

    // Subscribe to market data (quotes + trades)
    await this.sendMdRequest('md/subscribeQuote', { symbol: contractId });
    await this.sendMdRequest('md/subscribeTrades', { symbol: contractId });
    
    console.log(`✓ Subscribed to ${symbol} (contract ID: ${contractId})`);
  }

  async unsubscribeFromSymbol(symbol: string): Promise<void> {
    const contractId = this.contractIds.get(symbol);
    if (!contractId) return;

    await this.sendMdRequest('md/unsubscribeQuote', { symbol: contractId });
    await this.sendMdRequest('md/unsubscribeTrades', { symbol: contractId });
    
    this.contractIds.delete(symbol);
    this.quotes.delete(symbol);
  }

  private async getContractId(symbol: string): Promise<number> {
    // Query the contract endpoint to get the ID
    const response = await fetch(`${this.apiUrl}/contract/find?name=${symbol}`, {
      headers: {
        'Authorization': `Bearer ${this.accessToken}`,
        'Accept': 'application/json'
      }
    });

    if (!response.ok) {
      throw new Error(`Failed to find contract: ${symbol}`);
    }

    const contract = await response.json();
    
    if (!contract || !contract.id) {
      throw new Error(`Contract not found: ${symbol}`);
    }

    return contract.id;
  }

  onTrade(callback: (trade: Trade) => void): void {
    this.tradeCallbacks.push(callback);
  }

  onDisconnect(callback: () => void): void {
    this.disconnectCallbacks.push(callback);
  }

  disconnect(): void {
    if (this.mdSocket) {
      this.mdSocket.close();
      this.mdSocket = null;
    }
    this.accessToken = null;
    this.contractIds.clear();
    this.quotes.clear();
  }
}

// Contract symbol helpers
// NQ = Nasdaq 100 E-mini
// ES = S&P 500 E-mini
// Contract months: H=Mar, M=Jun, U=Sep, Z=Dec
// Example: NQH5 = NQ March 2025

export function getCurrentContractSymbol(root: 'NQ' | 'ES'): string {
  const now = new Date();
  const month = now.getMonth(); // 0-11
  const year = now.getFullYear() % 100; // Last 2 digits
  
  // Determine current front month
  // Contracts expire on 3rd Friday of contract month
  const contractMonths = [
    { month: 2, code: 'H' },  // March
    { month: 5, code: 'M' },  // June
    { month: 8, code: 'U' },  // September
    { month: 11, code: 'Z' }  // December
  ];

  for (let i = 0; i < contractMonths.length; i++) {
    const cm = contractMonths[i];
    if (month <= cm.month) {
      return `${root}${cm.code}${year}`;
    }
  }
  
  // If past December, use next year's March
  return `${root}H${year + 1}`;
}
