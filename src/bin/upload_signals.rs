use anyhow::Result;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use arrow::array::{Array, UInt64Array, StringArray, Float64Array};
use serde::Serialize;
use std::fs::File;

#[derive(Serialize)]
struct SignalRow {
    timestamp: i64,
    signal_type: String,
    direction: String,
    price: f64,
    strength: Option<String>,
    extra_data: Option<String>,
}

fn main() -> Result<()> {
    // Read parquet
    let file = File::open("output/signals.parquet")?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut signals: Vec<SignalRow> = Vec::new();

    for batch in reader {
        let batch = batch?;
        let timestamps = batch.column(0).as_any().downcast_ref::<UInt64Array>().unwrap();
        let signal_types = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
        let directions = batch.column(2).as_any().downcast_ref::<StringArray>().unwrap();
        let prices = batch.column(3).as_any().downcast_ref::<Float64Array>().unwrap();
        let strengths = batch.column(4).as_any().downcast_ref::<StringArray>().unwrap();
        let extra_data = batch.column(5).as_any().downcast_ref::<StringArray>().unwrap();

        for i in 0..batch.num_rows() {
            signals.push(SignalRow {
                timestamp: timestamps.value(i) as i64,
                signal_type: signal_types.value(i).to_string(),
                direction: directions.value(i).to_string(),
                price: prices.value(i),
                strength: if strengths.is_null(i) { None } else { Some(strengths.value(i).to_string()) },
                extra_data: if extra_data.is_null(i) { None } else { Some(extra_data.value(i).to_string()) },
            });
        }
    }

    // Output as JSON for curl upload
    println!("{}", serde_json::to_string(&signals)?);

    Ok(())
}
