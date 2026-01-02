//! Fetch historical data from Databento and precompute for backtesting

use anyhow::{Context, Result};
use chrono::{NaiveDate, Timelike};
use databento::{
    dbn::{SType, Schema, TradeMsg},
    historical::timeseries::GetRangeParams,
    HistoricalClient,
};
use time::{Date, Month, OffsetDateTime, Time, UtcOffset, macros::offset};
use tracing::info;

use crate::bars::{aggregate_to_1m_bars, aggregate_to_1s_bars};
use crate::impulse::detect_impulse_legs;
use crate::levels::compute_daily_levels;
use crate::lvn::extract_lvns;
use crate::precompute::DayData;
use crate::replay::replay_trades_for_signals;
use crate::trades::{Trade, Side};

const ET_OFFSET: UtcOffset = offset!(-5);

/// Fetch trades for a specific date and precompute all data
pub async fn fetch_and_precompute(
    api_key: &str,
    symbol: &str,
    date: NaiveDate,
) -> Result<DayData> {
    info!("Fetching trades from Databento for {}", date);

    let mut client = HistoricalClient::builder()
        .key(api_key)?
        .build()?;

    // Convert date
    let time_date = chrono_to_time_date(date)?;

    // Full trading day: fetch from midnight to 4pm ET (RTH close)
    // This avoids issues with data not yet available for late night hours
    let start = OffsetDateTime::new_in_offset(
        time_date,
        Time::from_hms(0, 0, 0)?,
        ET_OFFSET,
    );

    // Use 5pm ET as end (after RTH close), or current time if today
    let now_utc = chrono::Utc::now();
    let now_et = now_utc.with_timezone(&chrono_tz::America::New_York);
    let is_today = date == now_et.date_naive();

    let end = if is_today {
        // For today, use current ET time minus 30 minutes (but cap at 4pm ET to get RTH data)
        let current_et_hour = now_et.hour() as u8;
        let current_et_minute = now_et.minute() as u8;

        // Cap at 16:00 ET (4pm) to ensure we have full RTH
        let (hour, minute) = if current_et_hour > 16 || (current_et_hour == 16 && current_et_minute > 0) {
            (16u8, 0u8)
        } else if current_et_hour == 0 {
            // Very early morning, use previous day's close
            (16u8, 0u8)
        } else {
            // Use 30 minutes ago
            let mins_back = 30i32;
            let total_mins = (current_et_hour as i32) * 60 + (current_et_minute as i32) - mins_back;
            if total_mins < 0 {
                (0u8, 0u8)
            } else {
                ((total_mins / 60) as u8, (total_mins % 60) as u8)
            }
        };

        info!("Today's data - fetching up to {:02}:{:02} ET", hour, minute);
        OffsetDateTime::new_in_offset(
            time_date,
            Time::from_hms(hour, minute, 0)?,
            ET_OFFSET,
        )
    } else {
        // For historical dates, fetch full day until 5pm ET
        OffsetDateTime::new_in_offset(
            time_date,
            Time::from_hms(17, 0, 0)?,  // 5pm ET
            ET_OFFSET,
        )
    };

    info!("Fetching {} to {}", start, end);

    let trades = fetch_trades(&mut client, symbol, start, end).await?;
    info!("Fetched {} trades", trades.len());

    if trades.is_empty() {
        return Ok(DayData {
            date: date.format("%Y%m%d").to_string(),
            bars_1s: vec![],
            lvn_levels: vec![],
            daily_levels: vec![],
            signals: vec![],
        });
    }

    // Aggregate to bars
    let bars_1s = aggregate_to_1s_bars(&trades);
    let bars_1m = aggregate_to_1m_bars(&bars_1s);
    info!("Aggregated to {} 1s bars, {} 1m bars", bars_1s.len(), bars_1m.len());

    // Compute levels
    let daily_levels = compute_daily_levels(&bars_1s);

    // Detect impulse legs and LVNs
    let impulse_legs = detect_impulse_legs(&bars_1m, &daily_levels);
    let lvn_levels = extract_lvns(&trades, &impulse_legs);
    info!("Detected {} impulse legs, {} LVNs", impulse_legs.len(), lvn_levels.len());

    // Generate signals
    let signals = replay_trades_for_signals(&trades);

    Ok(DayData {
        date: date.format("%Y%m%d").to_string(),
        bars_1s,
        lvn_levels,
        daily_levels,
        signals,
    })
}

/// Convert chrono NaiveDate to time::Date
fn chrono_to_time_date(date: NaiveDate) -> Result<Date> {
    use chrono::Datelike;
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

/// Fetch trades from Databento historical API
async fn fetch_trades(
    client: &mut HistoricalClient,
    symbol: &str,
    start: OffsetDateTime,
    end: OffsetDateTime,
) -> Result<Vec<Trade>> {
    let mut decoder = client
        .timeseries()
        .get_range(
            &GetRangeParams::builder()
                .dataset("GLBX.MDP3")
                .date_time_range((start, end))
                .symbols(symbol)
                .stype_in(SType::RawSymbol)
                .schema(Schema::Trades)
                .build(),
        )
        .await
        .context("Failed to fetch from Databento")?;

    let mut trades = Vec::new();

    while let Some(record) = decoder.decode_record::<TradeMsg>().await? {
        let price = record.price as f64 / 1_000_000_000.0;
        let size = record.size;
        let ts_event = record.hd.ts_event;

        // Convert nanoseconds to DateTime<Utc>
        let ts = chrono::DateTime::from_timestamp_nanos(ts_event as i64);

        // Determine aggressor side from record.side
        // 'A' = Ask (buy aggressor), 'B' = Bid (sell aggressor)
        let side = if record.side == b'A' as i8 {
            Side::Buy
        } else {
            Side::Sell
        };

        trades.push(Trade {
            price,
            size: size as u64,
            ts_event: ts,
            side,
            symbol: symbol.to_string(),
        });
    }

    Ok(trades)
}
