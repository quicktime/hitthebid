import { createClient } from '@supabase/supabase-js';

const supabaseUrl = import.meta.env.VITE_SUPABASE_URL;
const supabaseAnonKey = import.meta.env.VITE_SUPABASE_ANON_KEY;

// Check if Supabase is configured
export const isSupabaseConfigured = !!(supabaseUrl && supabaseAnonKey);

// Create client (or null if not configured)
export const supabase = isSupabaseConfigured
  ? createClient(supabaseUrl, supabaseAnonKey)
  : null;

// Type definitions for database tables
export interface DbTradingSession {
  id: string;
  user_id: string;
  date: string;
  pdh: number | null;
  pdl: number | null;
  pdc: number | null;
  onh: number | null;
  onl: number | null;
  poc: number | null;
  vah: number | null;
  val: number | null;
  lvn_levels: number[] | null;
  premarket_bias: 'bullish' | 'bearish' | 'neutral';
  market_state_at_open: 'balance' | 'imbalance' | null;
  daily_thesis: string | null;
  daily_review: string | null;
  tomorrow_focus: string | null;
  lessons_learned: string | null;
  total_trades: number;
  winners: number;
  losers: number;
  scratches: number;
  gross_profit: number;
  gross_loss: number;
  net_pnl: number;
  equity_high: number;
  max_drawdown_from_high: number;
  created_at: string;
  updated_at: string;
}

export interface DbTrade {
  id: string;
  session_id: string;
  user_id: string;
  trade_number: number;
  entry_time: string;
  exit_time: string | null;
  market_state: 'balance' | 'imbalance';
  location_type: string;
  location_price: number;
  aggression_type: string;
  prism_confirmation: boolean;
  setup_grade: 'A' | 'B' | 'C';
  direction: 'long' | 'short';
  entry_price: number;
  stop_price: number;
  target_price: number;
  position_size: number;
  planned_rr: number | null;
  exit_price: number | null;
  exit_type: string | null;
  pnl: number | null;
  pnl_points: number | null;
  actual_rr: number | null;
  is_open: boolean;
  entry_notes: string | null;
  exit_notes: string | null;
  what_worked: string | null;
  what_to_improve: string | null;
  screenshot: string | null;
  signal_source: 'manual' | 'paper_trade' | 'backtest' | null;
  signal_id: string | null;
  created_at: string;
  updated_at: string;
}
