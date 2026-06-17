//! Minimal stdout demo: build a tagged series, convert it to an Arrow
//! RecordBatch, round-trip it through an IPC stream, and read it back.

use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_arrow::{batch_to_series, read_ipc, series_to_batch, write_ipc};

fn main() {
    let meta = TsSeriesMetadata::new(1, "cpu").with_tag("host", "edge-01");
    let mut series = TsSeries::<f64>::new();
    series.push(1_780_000_000_000_000_000, 0.42).unwrap();
    series.push(1_780_000_001_000_000_000, 0.55).unwrap();
    let series = series.with_metadata(meta);

    let batch = series_to_batch(&series).unwrap();
    println!(
        "batch: {} rows x {} cols",
        batch.num_rows(),
        batch.num_columns()
    );

    let ipc = write_ipc(&batch).unwrap();
    println!("ipc stream: {} bytes", ipc.len());

    let back = batch_to_series(&read_ipc(&ipc).unwrap()).unwrap();
    println!(
        "read back {} ({} points, last {:?})",
        back.metadata().map(|m| m.name.as_str()).unwrap_or("?"),
        back.len(),
        back.last().map(|p| p.value)
    );
}
