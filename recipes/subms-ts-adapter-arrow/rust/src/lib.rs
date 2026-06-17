//! `subms-ts-adapter-arrow` - an Apache Arrow adapter for the submillisecond cookbook
//! timeseries arc.
//!
//! Maps a `TsSeries<f64>` to a two-column `RecordBatch` (`ts: Int64`,
//! `v: Float64`) and a `TsCollection<f64>` to the tidy long-format batch
//! (`sid`, `ts`, `v`), with series identity carried in schema metadata. Both
//! round-trip through the Arrow IPC stream format, so the data hands off to
//! Polars / DuckDB / pandas with no translation layer.
//!
//! The columnar build is a bulk buffer fill, not per-element allocation, so
//! converting a whole series to a batch is the per-op sub-ms claim. The IPC
//! stream is Arrow-spec-compliant and cross-readable by any Arrow consumer;
//! byte-identical IPC across independent Arrow implementations is NOT claimed
//! (the format carries implementation-defined padding + metadata ordering).

mod convert;
mod error;

#[cfg(feature = "harness")]
pub mod recipe;

pub use convert::{
    batch_to_collection, batch_to_series, collection_to_batch, read_ipc, series_to_batch, write_ipc,
};
pub use error::TsArrowError;
