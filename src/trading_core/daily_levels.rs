//! Daily Levels Computation from Databento Historical Data
//!
//! Fetches yesterday's data and computes:
//! - PDH/PDL: Prior Day High/Low (RTH session 9:30am-4pm ET)
//! - POC/VAH/VAL: Point of Control, Value Area High/Low
//! - ONH/ONL: Overnight High/Low (6pm-9:30am ET)

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate, Timelike};
use chrono_tz::America::New_York;
use databento::{
    dbn::{SType, Schema, TradeMsg},
    HistoricalClient,
};
use std::collections::HashMap;
use time::{macros::offset, Date, Month, OffsetDateTime, Time, UtcOffset};
use tracing::info;

use super::state_machine::LiveDailyLevels;

/// RTH session times (Eastern Time)
const RTH_START_HOUR: u8 = 9;
const RTH_START_MIN: u8 = 30;
const RTH_END_HOUR: u8 = 16;
const RTH_END_MIN: u8 = 0;

/// Overnight session: 6pm to 9:30am next day
const ON_START_HOUR: u8 = 18;
const ON_END_HOUR: u8 = 9;
const ON_END_MIN: u8 = 30;

/// Price bucket size for volume profile
const PRICE_BUCKET_SIZE: f64 = 1.0;

/// Eastern Time offset (standard time is -5, DST is -4)
/// For simplicity, we'll use -5 (EST) and handle DST via chrono for date logic
const ET_OFFSET: UtcOffset = offset!(-5);

/// Fetch yesterday's data and compute daily levels
pub async fn fetch_daily_levels(
    api_key: &str,
    symbol: &str,
) -> Result<LiveDailyLevels> {
    info!("Fetching daily levels for {} from Databento...", symbol);

    let mut client = HistoricalClient::builder()
        .key(api_key)?
        .build()?;

    // Get current time in ET using chrono for proper timezone handling
    let now_utc = chrono::Utc::now();
    let now_et = now_utc.with_timezone(&New_York);
    let today = now_et.date_naive();

    // If it's before RTH close (4pm ET), use day before yesterday for "prior day"
    // If it's after RTH close, use yesterday
    let yesterday = if now_et.hour() < RTH_END_HOUR as u32 {
        today - chrono::Duration::days(2)
    } else {
        today - chrono::Duration::days(1)
    };

    // Skip weekends
    let yesterday = skip_weekends_backward(yesterday);

    info!("Computing levels from {}", yesterday);

    // Convert chrono NaiveDate to time crate's Date
    let time_yesterday = chrono_to_time_date(yesterday)?;
    let time_today = chrono_to_time_date(yesterday + chrono::Duration::days(1))?;

    // Create RTH session times using time crate
    let rth_start = OffsetDateTime::new_in_offset(
        time_yesterday,
        Time::from_hms(RTH_START_HOUR, RTH_START_MIN, 0)?,
        ET_OFFSET,
    );
    let rth_end = OffsetDateTime::new_in_offset(
        time_yesterday,
        Time::from_hms(RTH_END_HOUR, RTH_END_MIN, 0)?,
        ET_OFFSET,
    );

    // Overnight session: 6pm yesterday to 9:30am today
    let on_start = OffsetDateTime::new_in_offset(
        time_yesterday,
        Time::from_hms(ON_START_HOUR, 0, 0)?,
        ET_OFFSET,
    );
    let on_end = OffsetDateTime::new_in_offset(
        time_today,
        Time::from_hms(ON_END_HOUR, ON_END_MIN, 0)?,
        ET_OFFSET,
    );

    // Fetch RTH trades
    info!("Fetching RTH session: {} to {}", rth_start, rth_end);
    let rth_trades = fetch_trades(&mut client, symbol, rth_start, rth_end).await?;
    info!("Fetched {} RTH trades", rth_trades.len());

    // Fetch overnight trades
    info!("Fetching overnight session: {} to {}", on_start, on_end);
    let on_trades = fetch_trades(&mut client, symbol, on_start, on_end).await?;
    info!("Fetched {} overnight trades", on_trades.len());

    // Compute levels
    let (pdh, pdl, _session_open, _session_close) = compute_high_low_open_close(&rth_trades);
    let (poc, vah, val) = compute_volume_profile(&rth_trades);
    let (onh, onl) = if on_trades.is_empty() {
        (0.0, 0.0)
    } else {
        let (h, l, _, _) = compute_high_low_open_close(&on_trades);
        (h, l)
    };

    info!(
        "Computed levels: PDH={:.2}, PDL={:.2}, POC={:.2}, VAH={:.2}, VAL={:.2}, ONH={:.2}, ONL={:.2}",
        pdh, pdl, poc, vah, val, onh, onl
    );

    Ok(LiveDailyLevels {
        date: yesterday,
        pdh,
        pdl,
        onh,
        onl,
        poc,
        vah,
        val,
        session_high: pdh,
        session_low: pdl,
    })
}

/// Convert chrono NaiveDate to time::Date
fn chrono_to_time_date(date: NaiveDate) -> Result<Date> {
    let month = match date.month() {
        1 => Month::January,
        2 => Month::February,
        3 => Month::March,
        4 => Month::April,
        5 => Month::May,
        6 => Month::June,
        7 => Month::July,
        8 => Month::August,
        9 => Month::September,
        10 => Month::October,
        11 => Month::November,
        12 => Month::December,
        _ => return Err(anyhow::anyhow!("Invalid month")),
    };
    Date::from_calendar_date(date.year(), month, date.day() as u8)
        .map_err(|e| anyhow::anyhow!("Date conversion error: {}", e))
}

/// Skip weekends going backward
fn skip_weekends_backward(mut date: NaiveDate) -> NaiveDate {
    use chrono::Weekday;
    while date.weekday() == Weekday::Sat || date.weekday() == Weekday::Sun {
        date = date - chrono::Duration::days(1);
    }
    date
}

/// Simple trade struct for internal processing
struct SimpleTrade {
    price: f64,
    size: u64,
}

/// Fetch trades from Databento historical API
async fn fetch_trades(
    client: &mut HistoricalClient,
    symbol: &str,
    start: OffsetDateTime,
    end: OffsetDateTime,
) -> Result<Vec<SimpleTrade>> {
    let mut decoder = client
        .timeseries()
        .get_range(
            &databento::historical::timeseries::GetRangeParams::builder()
                .dataset("GLBX.MDP3")
                .date_time_range((start, end))
                .symbols(symbol)
                .schema(Schema::Trades)
                .stype_in(SType::RawSymbol)
                .build(),
        )
        .await
        .context("Failed to fetch historical data")?;

    let mut trades = Vec::new();

    while let Some(record) = decoder.decode_record::<TradeMsg>().await? {
        let price = record.price as f64 / 1_000_000_000.0;
        trades.push(SimpleTrade {
            price,
            size: record.size as u64,
        });
    }

    Ok(trades)
}

/// Compute high, low, open, close from trades
fn compute_high_low_open_close(trades: &[SimpleTrade]) -> (f64, f64, f64, f64) {
    if trades.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let mut high = f64::MIN;
    let mut low = f64::MAX;
    let open = trades.first().map(|t| t.price).unwrap_or(0.0);
    let close = trades.last().map(|t| t.price).unwrap_or(0.0);

    for trade in trades {
        high = high.max(trade.price);
        low = low.min(trade.price);
    }

    (high, low, open, close)
}

/// Compute POC, VAH, VAL from volume profile
fn compute_volume_profile(trades: &[SimpleTrade]) -> (f64, f64, f64) {
    if trades.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    // Build volume at price histogram
    let mut volume_at_price: HashMap<i64, u64> = HashMap::new();

    for trade in trades {
        let bucket = (trade.price / PRICE_BUCKET_SIZE).round() as i64;
        *volume_at_price.entry(bucket).or_insert(0) += trade.size;
    }

    if volume_at_price.is_empty() {
        let price = trades.first().map(|t| t.price).unwrap_or(0.0);
        return (price, price, price);
    }

    // Find POC (bucket with max volume)
    let (poc_bucket, _) = volume_at_price
        .iter()
        .max_by_key(|(_, vol)| *vol)
        .unwrap();
    let poc = *poc_bucket as f64 * PRICE_BUCKET_SIZE;

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

    let val = *sorted_buckets[val_idx].0 as f64 * PRICE_BUCKET_SIZE;
    let vah = *sorted_buckets[vah_idx].0 as f64 * PRICE_BUCKET_SIZE;

    (poc, vah, val)
}
