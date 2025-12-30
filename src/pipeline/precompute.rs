use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

use crate::bars::{aggregate_to_1m_bars, aggregate_to_1s_bars, Bar};
use crate::impulse::detect_impulse_legs;
use crate::levels::{compute_daily_levels, DailyLevels};
use crate::lvn::{extract_lvns, LvnLevel};
use crate::replay::{replay_trades_for_signals, CapturedSignal};
use crate::trades::parse_zst_trades;

/// Pre-computed data for a single trading day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayData {
    pub date: String,
    pub bars_1s: Vec<Bar>,
    pub lvn_levels: Vec<LvnLevel>,
    pub daily_levels: Vec<DailyLevels>,
    pub signals: Vec<CapturedSignal>,
}

/// Process a single day's data
pub fn process_day(zst_path: &Path) -> Result<DayData> {
    let filename = zst_path.file_name().unwrap().to_string_lossy();

    // Extract date from filename (glbx-mdp3-20251202.trades.csv.zst)
    let date = filename
        .split('-')
        .nth(2)
        .and_then(|s| s.split('.').next())
        .unwrap_or("unknown")
        .to_string();

    let trades = parse_zst_trades(zst_path)?;

    if trades.is_empty() {
        return Ok(DayData {
            date,
            bars_1s: vec![],
            lvn_levels: vec![],
            daily_levels: vec![],
            signals: vec![],
        });
    }

    // Create bars
    let bars_1s = aggregate_to_1s_bars(&trades);
    let bars_1m = aggregate_to_1m_bars(&bars_1s);

    // Compute levels
    let daily_levels = compute_daily_levels(&bars_1s);

    // Detect impulse legs and LVNs
    let impulse_legs = detect_impulse_legs(&bars_1m, &daily_levels);
    let lvn_levels = extract_lvns(&trades, &impulse_legs);

    // Generate signals (this is the slow part we want to cache)
    let signals = replay_trades_for_signals(&trades);

    Ok(DayData {
        date,
        bars_1s,
        lvn_levels,
        daily_levels,
        signals,
    })
}

/// Process multiple days in parallel
pub fn process_days_parallel(zst_files: &[PathBuf]) -> Vec<Result<DayData>> {
    zst_files
        .par_iter()
        .map(|path| {
            let result = process_day(path);
            if let Ok(ref data) = result {
                info!(
                    "Processed {}: {} bars, {} LVNs, {} signals",
                    data.date,
                    data.bars_1s.len(),
                    data.lvn_levels.len(),
                    data.signals.len()
                );
            }
            result
        })
        .collect()
}

/// Save precomputed data to cache directory
pub fn save_day_cache(data: &DayData, cache_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(format!("{}.json.zst", data.date));

    let json = serde_json::to_vec(data)?;
    let compressed = zstd::encode_all(&json[..], 3)?;
    std::fs::write(&path, compressed)?;

    Ok(())
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

/// Extract date from zst filename
pub fn extract_date_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_string_lossy();
    filename
        .split('-')
        .nth(2)
        .and_then(|s| s.split('.').next())
        .map(|s| s.to_string())
}

/// Load all cached data from cache directory
/// date_filter can be:
/// - Single date: "20250915"
/// - Month prefix: "202509"
/// - Date range: "20250901-20251120" (inclusive)
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

    // Load in parallel
    let results: Vec<_> = dates_to_load
        .par_iter()
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
