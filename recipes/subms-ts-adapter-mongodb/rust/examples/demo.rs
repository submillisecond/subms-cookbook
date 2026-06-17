//! Minimal stdout demo: build a tagged series, write it through the in-memory
//! store, read it back, and show the captured change events. No server.

use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_mongodb::{InMemoryMongoStore, TsMongoAdapter};

fn main() {
    let meta = TsSeriesMetadata::new(7, "cpu")
        .with_tag("host", "edge-01")
        .with_tag("region", "us-east-1");
    let mut series = TsSeries::<f64>::new();
    series.push(1_780_000_000_000_000_000, 0.42).unwrap();
    series.push(1_780_000_001_000_000_000, 0.55).unwrap();
    let series = series.with_metadata(meta);

    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    let n = adapter.write_series(&series).unwrap();
    println!("wrote {n} point documents");

    adapter.ensure_indexes().unwrap();
    let back = adapter.read_series(7).unwrap();
    println!(
        "read back {} ({} points, last {:?})",
        back.metadata().map(|m| m.name.as_str()).unwrap_or("?"),
        back.len(),
        back.last().map(|p| p.value)
    );

    let changes = adapter.poll_changes().unwrap();
    println!("captured {} change events", changes.len());
}
