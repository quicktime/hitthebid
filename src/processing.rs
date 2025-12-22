use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::info;

use crate::types::{
    AbsorptionEvent, AbsorptionZone, Bubble, CVDPoint, Trade, VolumeProfileLevel, WsMessage,
};

/// Volume snapshot for rolling average calculation
#[derive(Debug, Clone)]
struct VolumeSnapshot {
    timestamp: u64,
    volume: u32,
    #[allow(dead_code)]
    delta: i64,
}

/// Internal absorption zone tracking (more fields than we send to client)
#[derive(Debug, Clone)]
struct AbsorptionZoneInternal {
    price: f64,
    #[allow(dead_code)]
    price_key: i64,
    absorption_type: String,
    total_absorbed: i64,
    event_count: u32,
    first_seen: u64,
    last_seen: u64,
    peak_strength: u8, // 0=weak, 1=medium, 2=strong, 3=defended - never goes down
}

/// Processing state for trade aggregation
pub struct ProcessingState {
    trade_buffer: Vec<Trade>,
    bubble_counter: u64,
    cvd: i64,
    volume_profile: HashMap<i64, VolumeProfileLevel>, // Key = price * 4 (for 0.25 tick size)
    total_buy_volume: u64,
    total_sell_volume: u64,

    // Enhanced absorption detection
    window_first_price: Option<f64>,
    window_last_price: Option<f64>,

    // Rolling volume for dynamic thresholds (last 60 seconds)
    volume_history: Vec<VolumeSnapshot>,

    // Absorption zones by price level (key = price * 4)
    absorption_zones: HashMap<i64, AbsorptionZoneInternal>,

    // CVD trend tracking (for context)
    cvd_5s_ago: i64, // CVD from 5 seconds ago for trend detection
    cvd_history: Vec<(u64, i64)>, // (timestamp, cvd) for trend calculation
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            trade_buffer: Vec::new(),
            bubble_counter: 0,
            cvd: 0,
            volume_profile: HashMap::new(),
            total_buy_volume: 0,
            total_sell_volume: 0,
            window_first_price: None,
            window_last_price: None,
            volume_history: Vec::new(),
            absorption_zones: HashMap::new(),
            cvd_5s_ago: 0,
            cvd_history: Vec::new(),
        }
    }

    /// Calculate rolling average volume per second over last N seconds
    fn get_avg_volume_per_second(&self, seconds: u64) -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let cutoff = now.saturating_sub(seconds * 1000);

        let recent: Vec<_> = self
            .volume_history
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect();

        if recent.is_empty() {
            return 200.0; // Default baseline for NQ
        }

        let total_vol: u32 = recent.iter().map(|s| s.volume).sum();
        total_vol as f64 / seconds as f64
    }

    /// Get CVD trend direction: positive = bullish, negative = bearish
    fn get_cvd_trend(&self) -> i64 {
        self.cvd - self.cvd_5s_ago
    }

    /// Find POC (Point of Control) - price with highest volume
    fn get_poc(&self) -> Option<f64> {
        self.volume_profile
            .values()
            .max_by_key(|l| l.total_volume)
            .map(|l| l.price)
    }

    /// Get VAH/VAL (Value Area High/Low) - 70% of volume
    fn get_value_area(&self) -> Option<(f64, f64)> {
        if self.volume_profile.is_empty() {
            return None;
        }

        let total_vol: u32 = self.volume_profile.values().map(|l| l.total_volume).sum();
        let target_vol = (total_vol as f64 * 0.7) as u32;

        let poc = self.get_poc()?;
        let poc_key = (poc * 4.0).round() as i64;

        let mut included_vol = self.volume_profile.get(&poc_key)?.total_volume;
        let mut high_key = poc_key;
        let mut low_key = poc_key;

        // Expand outward from POC until we have 70% of volume
        while included_vol < target_vol {
            let above_key = high_key + 1;
            let below_key = low_key - 1;

            let above_vol = self
                .volume_profile
                .get(&above_key)
                .map(|l| l.total_volume)
                .unwrap_or(0);
            let below_vol = self
                .volume_profile
                .get(&below_key)
                .map(|l| l.total_volume)
                .unwrap_or(0);

            if above_vol == 0 && below_vol == 0 {
                break;
            }

            if above_vol >= below_vol {
                high_key = above_key;
                included_vol += above_vol;
            } else {
                low_key = below_key;
                included_vol += below_vol;
            }
        }

        Some((high_key as f64 / 4.0, low_key as f64 / 4.0))
    }

    /// Check if price is at a key level (POC, VAH, VAL)
    fn is_at_key_level(&self, price: f64) -> (bool, bool, bool) {
        let poc = self.get_poc();
        let va = self.get_value_area();

        let tolerance = 0.5; // Within 2 ticks

        let at_poc = poc.map(|p| (price - p).abs() <= tolerance).unwrap_or(false);
        let at_vah = va
            .map(|(h, _)| (price - h).abs() <= tolerance)
            .unwrap_or(false);
        let at_val = va
            .map(|(_, l)| (price - l).abs() <= tolerance)
            .unwrap_or(false);

        (at_poc, at_vah, at_val)
    }

    /// Calculate strength based on event count and context - returns (string, numeric)
    fn calculate_strength_with_num(
        &self,
        event_count: u32,
        at_key_level: bool,
        against_trend: bool,
    ) -> (&'static str, u8) {
        let base_strength = match event_count {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => 3,
        };

        let bonus = (if at_key_level { 1 } else { 0 }) + (if against_trend { 1 } else { 0 });
        let total = base_strength + bonus;

        match total {
            0 => ("weak", 0),
            1 => ("medium", 1),
            2 => ("strong", 2),
            _ => ("defended", 3),
        }
    }

    /// Convert numeric strength to string
    fn strength_num_to_str(num: u8) -> &'static str {
        match num {
            0 => "weak",
            1 => "medium",
            2 => "strong",
            _ => "defended",
        }
    }

    /// Clean up old absorption zones with strength-based persistence (using peak_strength)
    /// - Weak/Medium (0-1): 5 minutes
    /// - Strong (2): 15 minutes
    /// - Defended (3): 30 minutes
    fn cleanup_old_zones(&mut self, now: u64) {
        self.absorption_zones.retain(|_, zone| {
            let max_age_ms = match zone.peak_strength {
                0..=1 => 5 * 60 * 1000,  // 5 minutes for weak/medium
                2 => 15 * 60 * 1000,     // 15 minutes for strong
                _ => 30 * 60 * 1000,     // 30 minutes for defended
            };
            let cutoff = now.saturating_sub(max_age_ms);
            zone.last_seen >= cutoff
        });
    }

    /// Clean up old volume history (older than 60 seconds)
    fn cleanup_volume_history(&mut self, now: u64) {
        let cutoff = now.saturating_sub(60 * 1000);
        self.volume_history.retain(|s| s.timestamp >= cutoff);
    }

    /// Clean up old CVD history (older than 30 seconds)
    fn cleanup_cvd_history(&mut self, now: u64) {
        let cutoff = now.saturating_sub(30 * 1000);
        self.cvd_history.retain(|(ts, _)| *ts >= cutoff);

        // Update cvd_5s_ago
        let target = now.saturating_sub(5000);
        self.cvd_5s_ago = self
            .cvd_history
            .iter()
            .filter(|(ts, _)| *ts <= target)
            .max_by_key(|(ts, _)| *ts)
            .map(|(_, cvd)| *cvd)
            .unwrap_or(self.cvd);
    }

    /// Add a trade to the processing buffer
    pub fn add_trade(&mut self, trade: Trade) {
        // Update CVD
        let delta = if trade.side == "buy" {
            trade.size as i64
        } else {
            -(trade.size as i64)
        };
        self.cvd += delta;

        // Update volume totals
        if trade.side == "buy" {
            self.total_buy_volume += trade.size as u64;
        } else {
            self.total_sell_volume += trade.size as u64;
        }

        // Track first and last price for absorption detection
        if self.window_first_price.is_none() {
            self.window_first_price = Some(trade.price);
        }
        self.window_last_price = Some(trade.price);

        // Update volume profile (0.25 tick size)
        let price_key = (trade.price * 4.0).round() as i64;
        let rounded_price = price_key as f64 / 4.0;

        self.volume_profile
            .entry(price_key)
            .and_modify(|level| {
                if trade.side == "buy" {
                    level.buy_volume += trade.size;
                } else {
                    level.sell_volume += trade.size;
                }
                level.total_volume += trade.size;
            })
            .or_insert(VolumeProfileLevel {
                price: rounded_price,
                buy_volume: if trade.side == "buy" { trade.size } else { 0 },
                sell_volume: if trade.side == "sell" { trade.size } else { 0 },
                total_volume: trade.size,
            });

        // Add to buffer for aggregation
        self.trade_buffer.push(trade);
    }

    /// Process the trade buffer and emit bubbles, CVD points, and absorption events
    pub fn process_buffer(&mut self, tx: &broadcast::Sender<WsMessage>) {
        if self.trade_buffer.is_empty() {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Cleanup old data
        self.cleanup_old_zones(now);
        self.cleanup_volume_history(now);
        self.cleanup_cvd_history(now);

        // Aggregate by side
        let mut total_buy_volume = 0u32;
        let mut total_sell_volume = 0u32;
        let mut buy_trades = Vec::new();
        let mut sell_trades = Vec::new();

        for trade in &self.trade_buffer {
            if trade.side == "buy" {
                total_buy_volume += trade.size;
                buy_trades.push(trade);
            } else {
                total_sell_volume += trade.size;
                sell_trades.push(trade);
            }
        }

        let total_volume = total_buy_volume + total_sell_volume;
        if total_volume == 0 {
            self.trade_buffer.clear();
            return;
        }

        // Calculate delta and determine dominant side
        let delta = total_buy_volume as i64 - total_sell_volume as i64;
        let dominant_side = if delta > 0 { "buy" } else { "sell" };
        let dominant_volume = if delta > 0 {
            total_buy_volume
        } else {
            total_sell_volume
        };

        // Calculate volume-weighted average price for dominant side
        let dominant_trades = if delta > 0 { &buy_trades } else { &sell_trades };
        let avg_price = if !dominant_trades.is_empty() {
            let weighted_sum: f64 = dominant_trades
                .iter()
                .map(|t| t.price * t.size as f64)
                .sum();
            let total_size: u32 = dominant_trades.iter().map(|t| t.size).sum();
            weighted_sum / total_size as f64
        } else {
            self.trade_buffer.iter().map(|t| t.price).sum::<f64>() / self.trade_buffer.len() as f64
        };

        // Store volume snapshot for rolling average
        self.volume_history.push(VolumeSnapshot {
            timestamp: now,
            volume: total_volume,
            delta,
        });

        // Store CVD for trend tracking
        self.cvd_history.push((now, self.cvd));

        // Determine if imbalance is significant (> 15% of total volume)
        let imbalance_ratio = delta.abs() as f64 / total_volume as f64;
        let is_significant_imbalance = imbalance_ratio > 0.15;

        // Create bubble
        let bubble = Bubble {
            id: format!("bubble-{}", self.bubble_counter),
            price: avg_price,
            size: dominant_volume,
            side: dominant_side.to_string(),
            timestamp: now,
            x: 0.92,
            opacity: 1.0,
            is_significant_imbalance,
        };

        self.bubble_counter += 1;

        // Send bubble
        let _ = tx.send(WsMessage::Bubble(bubble));

        // Send CVD point
        let cvd_point = CVDPoint {
            timestamp: now,
            value: self.cvd,
            x: 0.92,
        };
        let _ = tx.send(WsMessage::CVDPoint(cvd_point));

        // === ENHANCED ABSORPTION DETECTION ===
        if let (Some(first_price), Some(last_price)) =
            (self.window_first_price, self.window_last_price)
        {
            let price_change = last_price - first_price;
            let abs_delta = delta.abs();

            // Dynamic threshold based on rolling average volume
            // Absorption requires delta > 40% of average volume per second
            let avg_vol = self.get_avg_volume_per_second(30);
            let min_delta_threshold = (avg_vol * 0.4).max(20.0) as i64;

            // Price movement threshold - 1 tick (0.25 for NQ)
            const PRICE_THRESHOLD: f64 = 0.25;

            if abs_delta >= min_delta_threshold {
                // Check for absorption:
                // - Buying absorbed: delta > 0 but price didn't go up (or went down)
                // - Selling absorbed: delta < 0 but price didn't go down (or went up)
                let is_buying_absorbed = delta > 0 && price_change <= PRICE_THRESHOLD;
                let is_selling_absorbed = delta < 0 && price_change >= -PRICE_THRESHOLD;

                if is_buying_absorbed || is_selling_absorbed {
                    let absorption_type = if is_buying_absorbed {
                        "buying"
                    } else {
                        "selling"
                    };
                    let price_key = (avg_price * 4.0).round() as i64;

                    // Get context
                    let (at_poc, at_vah, at_val) = self.is_at_key_level(avg_price);
                    let at_key_level = at_poc || at_vah || at_val;
                    let cvd_trend = self.get_cvd_trend();

                    // Against trend: buying absorbed during bullish trend, or selling absorbed during bearish trend
                    let against_trend = (is_buying_absorbed && cvd_trend > 100)
                        || (is_selling_absorbed && cvd_trend < -100);

                    // Update or create absorption zone
                    let zone = self
                        .absorption_zones
                        .entry(price_key)
                        .or_insert_with(|| AbsorptionZoneInternal {
                            price: avg_price,
                            price_key,
                            absorption_type: absorption_type.to_string(),
                            total_absorbed: 0,
                            event_count: 0,
                            first_seen: now,
                            last_seen: now,
                            peak_strength: 0,
                        });

                    zone.total_absorbed += abs_delta;
                    zone.event_count += 1;
                    zone.last_seen = now;
                    zone.absorption_type = absorption_type.to_string();

                    // Copy values before releasing borrow
                    let zone_event_count = zone.event_count;
                    let zone_total_absorbed = zone.total_absorbed;
                    let zone_peak_strength = zone.peak_strength;

                    // Calculate current strength (now we don't hold mutable borrow)
                    let (strength, strength_num) =
                        self.calculate_strength_with_num(zone_event_count, at_key_level, against_trend);

                    // Update peak strength if current is higher (never goes down)
                    if strength_num > zone_peak_strength {
                        if let Some(z) = self.absorption_zones.get_mut(&price_key) {
                            z.peak_strength = strength_num;
                        }
                    }

                    // Only emit events for medium+ strength OR first event at key level
                    let should_emit = strength != "weak" || (zone_event_count == 1 && at_key_level);

                    if should_emit {
                        let absorption_event = AbsorptionEvent {
                            timestamp: now,
                            price: avg_price,
                            absorption_type: absorption_type.to_string(),
                            delta,
                            price_change,
                            strength: strength.to_string(),
                            event_count: zone_event_count,
                            total_absorbed: zone_total_absorbed,
                            at_key_level,
                            against_trend,
                            x: 0.92,
                        };

                        let _ = tx.send(WsMessage::Absorption(absorption_event));

                        let context_str = match (at_poc, at_vah, at_val, against_trend) {
                            (true, _, _, true) => "@ POC ⚠️ AGAINST TREND",
                            (true, _, _, false) => "@ POC",
                            (_, true, _, true) => "@ VAH ⚠️ AGAINST TREND",
                            (_, true, _, false) => "@ VAH",
                            (_, _, true, true) => "@ VAL ⚠️ AGAINST TREND",
                            (_, _, true, false) => "@ VAL",
                            (_, _, _, true) => "⚠️ AGAINST TREND",
                            _ => "",
                        };

                        info!(
                            "🛡️ ABSORPTION [{}]: {} absorbed at {:.2} | events={} total={}  {} {}",
                            strength.to_uppercase(),
                            absorption_type,
                            avg_price,
                            zone_event_count,
                            zone_total_absorbed,
                            if zone_event_count >= 3 {
                                "🔥 DEFENDED LEVEL"
                            } else {
                                ""
                            },
                            context_str
                        );
                    }
                }
            }
        }

        // Send absorption zones update (only active ones)
        let zones: Vec<AbsorptionZone> = self
            .absorption_zones
            .values()
            .filter(|z| z.event_count >= 2) // Only send zones with 2+ events
            .map(|z| {
                let (at_poc, at_vah, at_val) = self.is_at_key_level(z.price);
                let cvd_trend = self.get_cvd_trend();
                let against_trend = (z.absorption_type == "buying" && cvd_trend > 100)
                    || (z.absorption_type == "selling" && cvd_trend < -100);

                // Use peak_strength - once defended, always defended
                let strength = Self::strength_num_to_str(z.peak_strength);

                AbsorptionZone {
                    price: z.price,
                    absorption_type: z.absorption_type.clone(),
                    total_absorbed: z.total_absorbed,
                    event_count: z.event_count,
                    first_seen: z.first_seen,
                    last_seen: z.last_seen,
                    strength: strength.to_string(),
                    at_poc,
                    at_vah,
                    at_val,
                    against_trend,
                }
            })
            .collect();

        if !zones.is_empty() {
            let _ = tx.send(WsMessage::AbsorptionZones { zones });
        }

        // Reset window price tracking
        self.window_first_price = None;
        self.window_last_price = None;

        // Clear buffer
        self.trade_buffer.clear();

        info!(
            "Created bubble: {} aggression={} ({:.0}% imbalance) {}",
            dominant_side,
            dominant_volume,
            imbalance_ratio * 100.0,
            if is_significant_imbalance {
                "COLORED"
            } else {
                "grey"
            }
        );
    }

    /// Send the current volume profile to clients
    pub fn send_volume_profile(&self, tx: &broadcast::Sender<WsMessage>) {
        let levels: Vec<VolumeProfileLevel> = self.volume_profile.values().cloned().collect();
        let _ = tx.send(WsMessage::VolumeProfile { levels });
    }
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self::new()
    }
}
