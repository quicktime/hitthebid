//! Cache loading for precomputed trading data
//!
//! Loads precomputed LVN levels and daily levels from cache files
//! without requiring the full pipeline dependencies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

use super::bars::Bar;
use super::levels::DailyLevels;
use super::lvn::LvnLevel;

/// Pre-computed data for a single trading day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayData {
    pub date: String,
    pub bars_1s: Vec<Bar>,
    pub lvn_levels: Vec<LvnLevel>,
    pub daily_levels: Vec<DailyLevels>,
    // Note: signals field is omitted as it requires replay module
    #[serde(default)]
    pub signals: Vec<serde_json::Value>, // Generic to avoid dependency on replay module
}

/// Load precomputed data from cache
pub fn load_day_cache(date: &str, cache_dir: &Path) -> Result<Option<DayData>> {
    let path = cache_dir.join(format!("{}.json.zst", date));

    if !path.exists() {
        return Ok(None);
    }

    let compressed = std::fs::read(&path)?;
    let json = zstd::decode_all(&compressed[..])?;
    let data: DayData = serde_json::from_slice(&json)?;

    Ok(Some(data))
}

/// Check which dates have cached data
pub fn get_cached_dates(cache_dir: &Path) -> Result<Vec<String>> {
    if !cache_dir.exists() {
        return Ok(vec![]);
    }

    let mut dates = Vec::new();
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.ends_with(".json.zst") {
            if let Some(date) = filename.strip_suffix(".json.zst") {
                dates.push(date.to_string());
            }
        }
    }

    dates.sort();
    Ok(dates)
}

/// Load all cached data from cache directory
/// date_filter can be:
/// - Single date: "20250915"
/// - Month prefix: "202509"
/// - Date range: "20250901:20251120" (inclusive)
pub fn load_all_cached(cache_dir: &Path, date_filter: Option<&str>) -> Result<Vec<DayData>> {
    let cached_dates = get_cached_dates(cache_dir)?;

    let dates_to_load: Vec<_> = if let Some(filter) = date_filter {
        // Support date ranges with ":" separator (e.g., "20250901:20251120")
        if filter.contains(':') {
            let parts: Vec<&str> = filter.split(':').collect();
            if parts.len() == 2 {
                let start = parts[0];
                let end = parts[1];
                cached_dates.into_iter()
                    .filter(|d| d.as_str() >= start && d.as_str() <= end)
                    .collect()
            } else {
                cached_dates.into_iter().filter(|d| d.contains(filter)).collect()
            }
        } else {
            cached_dates.into_iter().filter(|d| d.contains(filter)).collect()
        }
    } else {
        cached_dates
    };

    info!("Loading {} cached days...", dates_to_load.len());

    // Load sequentially (rayon is only in pipeline)
    let results: Vec<_> = dates_to_load
        .iter()
        .filter_map(|date| {
            match load_day_cache(date, cache_dir) {
                Ok(Some(data)) => Some(data),
                Ok(None) => None,
                Err(e) => {
                    info!("Failed to load cache for {}: {}", date, e);
                    None
                }
            }
        })
        .collect();

    info!("Loaded {} days from cache", results.len());
    Ok(results)
}
