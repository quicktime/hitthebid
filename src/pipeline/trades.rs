use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Get the front month NQ contract symbol for a given date
///
/// NQ futures expiration schedule (3rd Friday of expiry month):
/// - H (March) - front month from mid-Dec to mid-Mar
/// - M (June) - front month from mid-Mar to mid-Jun
/// - U (September) - front month from mid-Jun to mid-Sep
/// - Z (December) - front month from mid-Sep to mid-Dec
///
/// For simplicity, we use calendar quarters:
/// - Jan-Mar → H (March contract)
/// - Apr-Jun → M (June contract)
/// - Jul-Sep → U (September contract)
/// - Oct-Dec → Z (December contract)
pub fn get_front_month_symbol(date: NaiveDate) -> String {
    let month = date.month();
    let year_digit = (date.year() % 10) as u8;

    let contract_month = match month {
        1..=3 => 'H',   // March
        4..=6 => 'M',   // June
        7..=9 => 'U',   // September
        10..=12 => 'Z', // December
        _ => unreachable!(),
    };

    format!("NQ{}{}", contract_month, year_digit)
}

/// Extract date from a zst filename like "glbx-mdp3-20250103.trades.csv.zst"
pub fn extract_date_from_filename(path: &Path) -> Option<NaiveDate> {
    let filename = path.file_name()?.to_string_lossy();

    // Find the date portion (8 digits)
    for part in filename.split(&['-', '.'][..]) {
        if part.len() == 8 && part.chars().all(|c| c.is_ascii_digit()) {
            return NaiveDate::parse_from_str(part, "%Y%m%d").ok();
        }
    }
    None
}

/// Raw trade from Databento CSV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub ts_event: DateTime<Utc>,
    pub price: f64,
    pub size: u64,
    pub side: Side,
    pub symbol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// CSV row structure matching Databento trades schema
#[derive(Debug, Deserialize)]
struct CsvRow {
    ts_recv: String,
    ts_event: String,
    rtype: u8,
    publisher_id: u32,
    instrument_id: u64,
    action: String,
    side: String,
    depth: u8,
    price: f64,
    size: u64,
    flags: u32,
    ts_in_delta: i64,
    sequence: u64,
    symbol: String,
}

/// Find all .zst files in directory, optionally filtered by date
pub fn find_zst_files(data_dir: &Path, date_filter: Option<&str>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in std::fs::read_dir(data_dir)
        .with_context(|| format!("Failed to read directory: {:?}", data_dir))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "zst") {
            if let Some(filter) = date_filter {
                let filename = path.file_name().unwrap().to_string_lossy();
                if !filename.contains(filter) {
                    continue;
                }
            }
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

/// Parse trades from a zstd-compressed CSV file
/// Automatically determines the front month contract from the filename date
pub fn parse_zst_trades(path: &Path) -> Result<Vec<Trade>> {
    // Extract date from filename to determine front month
    let date = extract_date_from_filename(path)
        .with_context(|| format!("Failed to extract date from filename: {:?}", path))?;

    let front_month = get_front_month_symbol(date);
    tracing::debug!("Loading trades for {} (front month: {})", date, front_month);

    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {:?}", path))?;

    let decoder = zstd::stream::Decoder::new(file)
        .with_context(|| format!("Failed to create zstd decoder for: {:?}", path))?;

    let reader = BufReader::new(decoder);
    let mut csv_reader = csv::Reader::from_reader(reader);

    let mut trades = Vec::new();
    let mut skipped_other_contracts = 0u64;

    for result in csv_reader.deserialize() {
        let row: CsvRow = result.with_context(|| "Failed to parse CSV row")?;

        // Only process trade actions
        if row.action != "T" {
            continue;
        }

        // Only process the front month contract for this date
        // Skip spreads, back months, and other symbols
        if row.symbol != front_month {
            if row.symbol.starts_with("NQ") && !row.symbol.contains('-') {
                skipped_other_contracts += 1;
            }
            continue;
        }

        let side = match row.side.as_str() {
            "B" => Side::Buy,
            "A" => Side::Sell,
            _ => continue, // Skip unknown sides
        };

        // Parse timestamp
        let ts_event = DateTime::parse_from_rfc3339(&row.ts_event)
            .with_context(|| format!("Failed to parse timestamp: {}", row.ts_event))?
            .with_timezone(&Utc);

        trades.push(Trade {
            ts_event,
            price: row.price,
            size: row.size,
            side,
            symbol: row.symbol,
        });
    }

    if skipped_other_contracts > 0 {
        tracing::debug!(
            "Skipped {} trades from non-front-month contracts",
            skipped_other_contracts
        );
    }

    Ok(trades)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_zst_files() {
        let dir = Path::new("data/NQ_11_23_2025-12_23_2025");
        if dir.exists() {
            let files = find_zst_files(dir, None).unwrap();
            assert!(!files.is_empty());
        }
    }
}
