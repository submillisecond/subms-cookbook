//! Minimal stdout demo: build a tagged series, persist it to Parquet bytes,
//! read it back.

use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_parquet::{parquet_to_series, series_to_parquet};

fn main() {
    let meta = TsSeriesMetadata::new(1, "cpu").with_tag("host", "edge-01");
    let mut series = TsSeries::<f64>::new();
    series.push(1_780_000_000_000_000_000, 0.42).unwrap();
    series.push(1_780_000_001_000_000_000, 0.55).unwrap();
    let series = series.with_metadata(meta);

    let bytes = series_to_parquet(&series).unwrap();
    println!("parquet file: {} bytes", bytes.len());

    let back = parquet_to_series(&bytes).unwrap();
    println!(
        "read back {} ({} points, last {:?})",
        back.metadata().map(|m| m.name.as_str()).unwrap_or("?"),
        back.len(),
        back.last().map(|p| p.value)
    );
}
