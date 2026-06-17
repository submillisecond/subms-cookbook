//! The mapping core: `TsSeries` / `TsCollection` <-> Apache Parquet bytes.
//!
//! Parquet is a columnar file format, so this recipe is thin: it reuses the
//! `subms-ts-adapter-arrow` bridge to turn a series into a `RecordBatch`, then hands the
//! batch to the Parquet `ArrowWriter`. The series identity carried in the Arrow
//! schema metadata is persisted into the Parquet key-value metadata (the
//! `ARROW:schema` entry) and restored on read.

use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use subms_ts::{TsCollection, TsSeries};
use subms_ts_arrow::{batch_to_collection, batch_to_series, collection_to_batch, series_to_batch};

use crate::error::TsParquetError;

// Large enough that our claim-size series / collections come back as a single
// batch; a file with more rows than this pages into multiple batches, which
// `read_batch` concatenates by re-reading.
const READ_BATCH_ROWS: usize = 1 << 20;

fn write_batch(batch: &RecordBatch) -> Result<Vec<u8>, TsParquetError> {
    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), None)
        .map_err(|e| TsParquetError::parquet(e.to_string()))?;
    writer
        .write(batch)
        .map_err(|e| TsParquetError::parquet(e.to_string()))?;
    writer
        .close()
        .map_err(|e| TsParquetError::parquet(e.to_string()))?;
    Ok(buf)
}

fn read_batch(bytes: &[u8]) -> Result<RecordBatch, TsParquetError> {
    let data = bytes::Bytes::copy_from_slice(bytes);
    let builder = ParquetRecordBatchReaderBuilder::try_new(data)
        .map_err(|e| TsParquetError::parquet(e.to_string()))?;
    // The builder schema keeps the file's schema metadata (subms.sid / name /
    // tags); the per-batch schema parquet-rs yields drops it, so re-attach it.
    let schema = builder.schema().clone();
    let reader = builder
        .with_batch_size(READ_BATCH_ROWS)
        .build()
        .map_err(|e| TsParquetError::parquet(e.to_string()))?;

    let mut batches = Vec::new();
    for b in reader {
        batches.push(b.map_err(|e| TsParquetError::parquet(e.to_string()))?);
    }
    let columns = match batches.len() {
        0 => return Ok(RecordBatch::new_empty(schema)),
        1 => batches.pop().unwrap().columns().to_vec(),
        _ => arrow::compute::concat_batches(&batches[0].schema(), &batches)
            .map_err(|e| TsParquetError::parquet(e.to_string()))?
            .columns()
            .to_vec(),
    };
    RecordBatch::try_new(schema, columns).map_err(|e| TsParquetError::parquet(e.to_string()))
}

/// Persist one series to Parquet bytes.
pub fn series_to_parquet(series: &TsSeries<f64>) -> Result<Vec<u8>, TsParquetError> {
    let batch = series_to_batch(series).map_err(|e| TsParquetError::arrow(e.to_string()))?;
    write_batch(&batch)
}

/// Read a series back from Parquet bytes.
pub fn parquet_to_series(bytes: &[u8]) -> Result<TsSeries<f64>, TsParquetError> {
    let batch = read_batch(bytes)?;
    batch_to_series(&batch).map_err(|e| TsParquetError::arrow(e.to_string()))
}

/// Persist a collection to Parquet bytes (the long-format `sid`, `ts`, `v`).
pub fn collection_to_parquet(coll: &TsCollection<f64>) -> Result<Vec<u8>, TsParquetError> {
    let batch = collection_to_batch(coll).map_err(|e| TsParquetError::arrow(e.to_string()))?;
    write_batch(&batch)
}

/// Read a collection back from Parquet bytes.
pub fn parquet_to_collection(bytes: &[u8]) -> Result<TsCollection<f64>, TsParquetError> {
    let batch = read_batch(bytes)?;
    batch_to_collection(&batch).map_err(|e| TsParquetError::arrow(e.to_string()))
}
