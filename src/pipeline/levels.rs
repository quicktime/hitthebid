use crate::bars::Bar;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Daily reference levels for a trading session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLevels {
    pub date: NaiveDate,
    pub symbol: String,

    // Prior day levels (for next day reference)
    pub pdh: f64, // Prior Day High
    pub pdl: f64, // Prior Day Low
    pub pdc: f64, // Prior Day Close

    // Overnight levels (pre-RTH session)
    pub onh: f64, // Overnight High
    pub onl: f64, // Overnight Low

    // Volume Profile levels (computed from current day)
    pub poc: f64, // Point of Control - price with highest volume
    pub vah: f64, // Value Area High - upper bound of 70% volume
    pub val: f64, // Value Area Low - lower bound of 70% volume

    // Session stats
    pub session_high: f64,
    pub session_low: f64,
    pub session_open: f64,
    pub session_close: f64,
    pub total_volume: u64,
}

/// Type of key level for trading signals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LevelType {
    POC,  // Point of Control
    VAH,  // Value Area High
    VAL,  // Value Area Low
    PDH,  // Prior Day High
    PDL,  // Prior Day Low
    ONH,  // Overnight High
    ONL,  // Overnight Low
    LVN,  // Low Volume Node
}

impl std::fmt::Display for LevelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelType::POC => write!(f, "POC"),
            LevelType::VAH => write!(f, "VAH"),
            LevelType::VAL => write!(f, "VAL"),
            LevelType::PDH => write!(f, "PDH"),
            LevelType::PDL => write!(f, "PDL"),
            LevelType::ONH => write!(f, "ONH"),
            LevelType::ONL => write!(f, "ONL"),
            LevelType::LVN => write!(f, "LVN"),
        }
    }
}

/// A key level with its type and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLevel {
    pub price: f64,
    pub level_type: LevelType,
    pub date: NaiveDate,
    /// Strength indicator (0.0-1.0, higher = stronger)
    /// For LVNs: 1.0 - volume_ratio (lower volume = higher strength)
    /// For other levels: 1.0 (always strong)
    pub strength: f64,
}

/// Index of all key levels for fast proximity lookup
pub struct LevelIndex {
    /// All levels sorted by price
    levels: Vec<KeyLevel>,
    /// Tolerance in points for "at level" detection
    tolerance: f64,
}

impl LevelIndex {
    /// Create a new LevelIndex from daily levels and LVNs
    pub fn new(
        daily_levels: &[DailyLevels],
        lvn_levels: &[crate::lvn::LvnLevel],
        tolerance: f64,
    ) -> Self {
        let mut levels = Vec::new();

        // Add daily levels
        for dl in daily_levels {
            // POC, VAH, VAL
            levels.push(KeyLevel {
                price: dl.poc,
                level_type: LevelType::POC,
                date: dl.date,
                strength: 1.0,
            });
            levels.push(KeyLevel {
                price: dl.vah,
                level_type: LevelType::VAH,
                date: dl.date,
                strength: 1.0,
            });
            levels.push(KeyLevel {
                price: dl.val,
                level_type: LevelType::VAL,
                date: dl.date,
                strength: 1.0,
            });
            // PDH, PDL
            levels.push(KeyLevel {
                price: dl.pdh,
                level_type: LevelType::PDH,
                date: dl.date,
                strength: 1.0,
            });
            levels.push(KeyLevel {
                price: dl.pdl,
                level_type: LevelType::PDL,
                date: dl.date,
                strength: 1.0,
            });
            // ONH, ONL
            levels.push(KeyLevel {
                price: dl.onh,
                level_type: LevelType::ONH,
                date: dl.date,
                strength: 1.0,
            });
            levels.push(KeyLevel {
                price: dl.onl,
                level_type: LevelType::ONL,
                date: dl.date,
                strength: 1.0,
            });
        }

        // Add LVN levels (strength = inverse of volume ratio)
        for lvn in lvn_levels {
            let strength = 1.0 - lvn.volume_ratio.min(1.0);
            levels.push(KeyLevel {
                price: lvn.price,
                level_type: LevelType::LVN,
                date: lvn.date,
                strength,
            });
        }

        // Sort by price for efficient searching
        levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap_or(std::cmp::Ordering::Equal));

        Self { levels, tolerance }
    }

    /// Find all levels within tolerance of a price on a specific date
    pub fn levels_near(&self, price: f64, date: NaiveDate) -> Vec<&KeyLevel> {
        self.levels
            .iter()
            .filter(|l| l.date == date && (l.price - price).abs() <= self.tolerance)
            .collect()
    }

    /// Check if price is at any key level on the given date
    pub fn is_at_level(&self, price: f64, date: NaiveDate) -> bool {
        !self.levels_near(price, date).is_empty()
    }

    /// Get the strongest level near price (for trade context)
    pub fn strongest_level_near(&self, price: f64, date: NaiveDate) -> Option<&KeyLevel> {
        self.levels_near(price, date)
            .into_iter()
            .max_by(|a, b| a.strength.partial_cmp(&b.strength).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Get all levels for a specific date
    pub fn levels_for_date(&self, date: NaiveDate) -> Vec<&KeyLevel> {
        self.levels.iter().filter(|l| l.date == date).collect()
    }

    /// Total number of levels in the index
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }
}

/// Trading session boundaries (CME NQ futures)
/// Regular Trading Hours: 9:30 AM - 4:00 PM ET (14:30 - 21:00 UTC)
/// Full session: 6:00 PM - 5:00 PM ET next day
const RTH_START_HOUR: u32 = 14; // 9:30 AM ET = 14:30 UTC
const RTH_START_MIN: u32 = 30;
const RTH_END_HOUR: u32 = 21; // 4:00 PM ET = 21:00 UTC

/// Price bucket size for volume profile (NQ tick = 0.25)
const PRICE_BUCKET_SIZE: f64 = 1.0; // 1 point buckets for cleaner profile

pub fn compute_daily_levels(bars: &[Bar]) -> Vec<DailyLevels> {
    if bars.is_empty() {
        return Vec::new();
    }

    // Group bars by trading date (use RTH session date)
    let mut daily_bars: BTreeMap<NaiveDate, Vec<&Bar>> = BTreeMap::new();

    for bar in bars {
        // Use the bar's date as the trading date
        // For proper session handling, we'd need to map overnight sessions
        let date = bar.timestamp.date_naive();
        daily_bars.entry(date).or_default().push(bar);
    }

    let mut levels_list = Vec::new();
    let dates: Vec<_> = daily_bars.keys().cloned().collect();

    for (i, date) in dates.iter().enumerate() {
        let bars = daily_bars.get(date).unwrap();
        if bars.is_empty() {
            continue;
        }

        let symbol = bars[0].symbol.clone();

        // Compute current day's session stats
        let session_high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let session_low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let session_open = bars.first().map(|b| b.open).unwrap_or(0.0);
        let session_close = bars.last().map(|b| b.close).unwrap_or(0.0);
        let total_volume: u64 = bars.iter().map(|b| b.volume).sum();

        // Get prior day levels (from previous day in our data)
        let (pdh, pdl, pdc) = if i > 0 {
            let prev_date = &dates[i - 1];
            let prev_bars = daily_bars.get(prev_date).unwrap();
            (
                prev_bars.iter().map(|b| b.high).fold(f64::MIN, f64::max),
                prev_bars.iter().map(|b| b.low).fold(f64::MAX, f64::min),
                prev_bars.last().map(|b| b.close).unwrap_or(0.0),
            )
        } else {
            // First day in dataset - use current day's open as reference
            (session_high, session_low, session_open)
        };

        // Compute volume profile
        let (poc, vah, val) = compute_volume_profile(bars);

        levels_list.push(DailyLevels {
            date: *date,
            symbol,
            pdh,
            pdl,
            pdc,
            onh: 0.0, // TODO: Compute from overnight session
            onl: 0.0, // TODO: Compute from overnight session
            poc,
            vah,
            val,
            session_high,
            session_low,
            session_open,
            session_close,
            total_volume,
        });
    }

    levels_list
}

/// Build volume profile and compute POC, VAH, VAL
fn compute_volume_profile(bars: &[&Bar]) -> (f64, f64, f64) {
    if bars.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    // Build volume at price histogram
    let mut volume_at_price: HashMap<i64, u64> = HashMap::new();

    for bar in bars {
        // Distribute bar volume across the bar's range
        // For simplicity, put all volume at VWAP-ish price (midpoint)
        let bar_mid = (bar.high + bar.low) / 2.0;
        let bucket = price_to_bucket(bar_mid);
        *volume_at_price.entry(bucket).or_insert(0) += bar.volume;
    }

    if volume_at_price.is_empty() {
        let price = bars[0].close;
        return (price, price, price);
    }

    // Find POC (bucket with max volume)
    let (poc_bucket, _) = volume_at_price
        .iter()
        .max_by_key(|(_, vol)| *vol)
        .unwrap();
    let poc = bucket_to_price(*poc_bucket);

    // Compute Value Area (70% of total volume)
    let total_volume: u64 = volume_at_price.values().sum();
    let target_volume = (total_volume as f64 * 0.70) as u64;

    // Sort buckets by price
    let mut sorted_buckets: Vec<_> = volume_at_price.iter().collect();
    sorted_buckets.sort_by_key(|(bucket, _)| *bucket);

    // Expand from POC to find value area
    let poc_idx = sorted_buckets
        .iter()
        .position(|(b, _)| *b == poc_bucket)
        .unwrap_or(0);

    let mut val_idx = poc_idx;
    let mut vah_idx = poc_idx;
    let mut accumulated_volume = *volume_at_price.get(poc_bucket).unwrap_or(&0);

    while accumulated_volume < target_volume {
        let can_go_lower = val_idx > 0;
        let can_go_higher = vah_idx < sorted_buckets.len() - 1;

        if !can_go_lower && !can_go_higher {
            break;
        }

        let lower_vol = if can_go_lower {
            *sorted_buckets[val_idx - 1].1
        } else {
            0
        };

        let upper_vol = if can_go_higher {
            *sorted_buckets[vah_idx + 1].1
        } else {
            0
        };

        if lower_vol >= upper_vol && can_go_lower {
            val_idx -= 1;
            accumulated_volume += lower_vol;
        } else if can_go_higher {
            vah_idx += 1;
            accumulated_volume += upper_vol;
        } else if can_go_lower {
            val_idx -= 1;
            accumulated_volume += lower_vol;
        }
    }

    let val = bucket_to_price(*sorted_buckets[val_idx].0);
    let vah = bucket_to_price(*sorted_buckets[vah_idx].0);

    (poc, vah, val)
}

fn price_to_bucket(price: f64) -> i64 {
    (price / PRICE_BUCKET_SIZE).round() as i64
}

fn bucket_to_price(bucket: i64) -> f64 {
    bucket as f64 * PRICE_BUCKET_SIZE
}

/// Check if a price is within a tolerance of a level
pub fn is_near_level(price: f64, level: f64, tolerance: f64) -> bool {
    (price - level).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_volume_profile() {
        let ts = Utc::now();
        let bars: Vec<&Bar> = vec![];
        // Would need actual bar data for meaningful test
    }
}
