use crate::impulse::{ImpulseDirection, ImpulseLeg};
use crate::trades::Trade;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Price bucket size for volume profile (finer granularity for LVN detection)
const LVN_BUCKET_SIZE: f64 = 0.5; // 2 ticks = 0.5 points for NQ

/// Threshold for LVN: volume < 15% of average volume at price (stricter = fewer, stronger LVNs)
const LVN_THRESHOLD_RATIO: f64 = 0.15;

/// Default UUID for backward compatibility with old cache files
fn default_uuid() -> Uuid {
    Uuid::nil()
}

/// Low Volume Node extracted from impulse leg volume profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LvnLevel {
    /// Links to the parent impulse leg that created this LVN
    #[serde(default = "default_uuid")]
    pub impulse_id: Uuid,
    pub price: f64,
    pub volume: u64,
    pub avg_volume: f64,
    pub volume_ratio: f64, // Actual/Average (< 0.3 qualifies)
    pub impulse_start_time: DateTime<Utc>,
    pub impulse_end_time: DateTime<Utc>,
    pub impulse_direction: ImpulseDirection, // Direction of impulse that created this LVN
    pub date: NaiveDate,
    pub symbol: String,
}

/// Extract LVNs from impulse legs by building volume profiles for each leg
pub fn extract_lvns(trades: &[Trade], impulse_legs: &[ImpulseLeg]) -> Vec<LvnLevel> {
    let mut lvn_levels = Vec::new();

    for leg in impulse_legs {
        // Filter trades within this impulse leg's time window
        let leg_trades: Vec<_> = trades
            .iter()
            .filter(|t| t.ts_event >= leg.start_time && t.ts_event <= leg.end_time)
            .collect();

        if leg_trades.is_empty() {
            continue;
        }

        // Build volume profile for this leg
        let mut volume_at_price: HashMap<i64, u64> = HashMap::new();

        for trade in &leg_trades {
            let bucket = price_to_bucket(trade.price);
            *volume_at_price.entry(bucket).or_insert(0) += trade.size;
        }

        if volume_at_price.is_empty() {
            continue;
        }

        // Calculate average volume across all price levels
        let total_volume: u64 = volume_at_price.values().sum();
        let avg_volume = total_volume as f64 / volume_at_price.len() as f64;

        // Find LVNs: price levels with volume < 30% of average
        for (bucket, volume) in &volume_at_price {
            let volume_ratio = *volume as f64 / avg_volume;

            if volume_ratio < LVN_THRESHOLD_RATIO {
                lvn_levels.push(LvnLevel {
                    impulse_id: leg.id,
                    price: bucket_to_price(*bucket),
                    volume: *volume,
                    avg_volume,
                    volume_ratio,
                    impulse_start_time: leg.start_time,
                    impulse_end_time: leg.end_time,
                    impulse_direction: leg.direction,
                    date: leg.date,
                    symbol: leg.symbol.clone(),
                });
            }
        }
    }

    // Sort by price
    lvn_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

    lvn_levels
}

fn price_to_bucket(price: f64) -> i64 {
    (price / LVN_BUCKET_SIZE).round() as i64
}

fn bucket_to_price(bucket: i64) -> f64 {
    bucket as f64 * LVN_BUCKET_SIZE
}

/// Extract LVNs from trades in real-time for a specific impulse leg
///
/// This function is used by the state machine to extract LVNs from trades
/// that occurred during an impulse leg. Unlike `extract_lvns`, this works
/// with individual trades and a specific time window, rather than pre-detected
/// impulse legs.
pub fn extract_lvns_realtime(
    trades: &[Trade],
    impulse_id: Uuid,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    direction: ImpulseDirection,
    symbol: &str,
) -> Vec<LvnLevel> {
    // Filter trades within the time window
    let impulse_trades: Vec<_> = trades
        .iter()
        .filter(|t| t.ts_event >= start_time && t.ts_event <= end_time)
        .collect();

    if impulse_trades.is_empty() {
        return Vec::new();
    }

    // Build volume profile
    let mut volume_at_price: HashMap<i64, u64> = HashMap::new();
    for trade in &impulse_trades {
        let bucket = price_to_bucket(trade.price);
        *volume_at_price.entry(bucket).or_insert(0) += trade.size;
    }

    if volume_at_price.is_empty() {
        return Vec::new();
    }

    // Calculate average volume across all price levels
    let total_volume: u64 = volume_at_price.values().sum();
    let avg_volume = total_volume as f64 / volume_at_price.len() as f64;

    // Find LVNs
    let mut lvn_levels = Vec::new();
    let date = start_time.date_naive();

    for (bucket, volume) in &volume_at_price {
        let volume_ratio = *volume as f64 / avg_volume;

        if volume_ratio < LVN_THRESHOLD_RATIO {
            lvn_levels.push(LvnLevel {
                impulse_id,
                price: bucket_to_price(*bucket),
                volume: *volume,
                avg_volume,
                volume_ratio,
                impulse_start_time: start_time,
                impulse_end_time: end_time,
                impulse_direction: direction,
                date,
                symbol: symbol.to_string(),
            });
        }
    }

    // Sort by price
    lvn_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

    lvn_levels
}

/// Extract LVNs from a full day's volume profile using 1s bars
///
/// Unlike impulse-based LVNs, this extracts LVNs from the ENTIRE day's volume
/// distribution. This approach is more stable because:
/// 1. No reliance on real-time impulse detection (which proved unreliable)
/// 2. LVNs represent actual low-volume gaps in the full session
/// 3. Can be pre-computed overnight for next-day trading
///
/// Returns LVNs sorted by price.
pub fn extract_lvns_from_full_profile(
    bars: &[crate::bars::Bar],
    min_session_hour_et: u32,  // e.g., 9 for 9:00 AM start
    max_session_hour_et: u32,  // e.g., 16 for 4:00 PM end (exclusive)
) -> Vec<LvnLevel> {
    use chrono_tz::America::New_York;
    use std::collections::HashMap;

    if bars.is_empty() {
        return Vec::new();
    }

    // Build volume profile from bars, distributing volume across each bar's range
    let mut volume_at_price: HashMap<i64, u64> = HashMap::new();

    let symbol = bars.first().map(|b| b.symbol.clone()).unwrap_or_default();
    let date = bars.first().map(|b| b.timestamp.date_naive()).unwrap_or_else(|| {
        chrono::Utc::now().date_naive()
    });

    for bar in bars {
        // Filter by trading hours
        let et_time = bar.timestamp.with_timezone(&New_York);
        let hour = et_time.hour();
        let minute = et_time.minute();

        // Check if within session hours
        let in_session = if min_session_hour_et == 9 && max_session_hour_et == 16 {
            // RTH: 9:30-16:00
            (hour > 9 || (hour == 9 && minute >= 30)) && hour < 16
        } else {
            hour >= min_session_hour_et && hour < max_session_hour_et
        };

        if !in_session {
            continue;
        }

        // Distribute volume across the bar's price range
        // Use OHLC to get key prices where volume actually occurred
        let prices = [bar.open, bar.high, bar.low, bar.close];
        let vol_per_level = bar.volume / 4;  // Distribute evenly across OHLC
        let remainder = bar.volume % 4;

        for (i, &price) in prices.iter().enumerate() {
            let bucket = price_to_bucket(price);
            let vol = if i == 0 { vol_per_level + remainder } else { vol_per_level };
            *volume_at_price.entry(bucket).or_insert(0) += vol;
        }
    }

    if volume_at_price.is_empty() {
        return Vec::new();
    }

    // Calculate average volume across all price levels
    let total_volume: u64 = volume_at_price.values().sum();
    let avg_volume = total_volume as f64 / volume_at_price.len() as f64;

    // Find LVNs: price levels with volume < threshold of average
    // For full profile, use a slightly higher threshold since it's the whole day
    let lvn_threshold = LVN_THRESHOLD_RATIO * 1.5; // 22.5% instead of 15%

    let mut lvn_levels = Vec::new();

    // We need to assign a direction to full-profile LVNs
    // Use the day's overall direction (close vs open)
    let first_price = bars.first().map(|b| b.open).unwrap_or(0.0);
    let last_price = bars.last().map(|b| b.close).unwrap_or(first_price);
    let day_direction = if last_price > first_price {
        ImpulseDirection::Up
    } else {
        ImpulseDirection::Down
    };

    // For full profile LVNs, we'll use the overall session times
    let first_bar = bars.iter().next();
    let last_bar = bars.iter().last();
    let start_time = first_bar.map(|b| b.timestamp).unwrap_or_else(chrono::Utc::now);
    let end_time = last_bar.map(|b| b.timestamp).unwrap_or(start_time);

    for (&bucket, &volume) in &volume_at_price {
        let volume_ratio = volume as f64 / avg_volume;

        if volume_ratio < lvn_threshold {
            let price = bucket_to_price(bucket);

            // Determine LVN direction based on where it is relative to session VWAP
            // LVN above VWAP = resistance (came from down impulse) → SHORT on retest
            // LVN below VWAP = support (came from up impulse) → LONG on retest
            let vwap = calculate_simple_vwap(bars);
            let lvn_direction = if price > vwap {
                ImpulseDirection::Down  // LVN is resistance
            } else {
                ImpulseDirection::Up    // LVN is support
            };

            lvn_levels.push(LvnLevel {
                impulse_id: Uuid::nil(), // No impulse for full-profile LVNs
                price,
                volume,
                avg_volume,
                volume_ratio,
                impulse_start_time: start_time,
                impulse_end_time: end_time,
                impulse_direction: lvn_direction,
                date,
                symbol: symbol.clone(),
            });
        }
    }

    // Sort by price
    lvn_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());

    lvn_levels
}

/// Calculate simple VWAP from bars
fn calculate_simple_vwap(bars: &[crate::bars::Bar]) -> f64 {
    if bars.is_empty() {
        return 0.0;
    }

    let mut volume_sum: u64 = 0;
    let mut pv_sum: f64 = 0.0;

    for bar in bars {
        let typical_price = (bar.high + bar.low + bar.close) / 3.0;
        pv_sum += typical_price * bar.volume as f64;
        volume_sum += bar.volume;
    }

    if volume_sum == 0 {
        return bars.last().map(|b| b.close).unwrap_or(0.0);
    }

    pv_sum / volume_sum as f64
}

/// Extract LVNs from RTH session (9:30-16:00 ET)
pub fn extract_lvns_from_rth_profile(bars: &[crate::bars::Bar]) -> Vec<LvnLevel> {
    extract_lvns_from_full_profile(bars, 9, 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lvn_bucket_conversion() {
        let price = 21500.5;
        let bucket = price_to_bucket(price);
        let recovered = bucket_to_price(bucket);
        assert!((price - recovered).abs() < 0.01);
    }
}
