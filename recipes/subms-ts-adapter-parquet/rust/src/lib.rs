//! `subms-ts-adapter-parquet` - an Apache Parquet adapter for the submillisecond
//! cookbook timeseries arc.
//!
//! Persists a `TsSeries<f64>` / `TsCollection<f64>` to Parquet bytes and reads
//! them back, readable by Spark / DuckDB / pandas / Polars. The mapping is thin:
//! it composes on `subms-ts-adapter-arrow` to turn a series into a columnar
//! `RecordBatch`, then writes that batch through the Parquet `ArrowWriter`.
//! Series identity rides in the Parquet key-value metadata and is restored on
//! read.

mod convert;
mod error;

#[cfg(feature = "harness")]
pub mod recipe;

pub use convert::{
    collection_to_parquet, parquet_to_collection, parquet_to_series, series_to_parquet,
};
pub use error::TsParquetError;
