use crate::impulse::{ImpulseDirection, ImpulseLeg};
use crate::trades::Trade;
use chrono::{DateTime, NaiveDate, Utc};
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
