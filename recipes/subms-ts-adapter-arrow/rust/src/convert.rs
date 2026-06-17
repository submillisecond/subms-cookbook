//! The pure mapping core: `TsSeries` / `TsCollection` <-> Arrow `RecordBatch`
//! and Arrow IPC streams.
//!
//! A single series maps to a two-column batch (`ts: Int64`, `v: Float64`) with
//! its identity carried in schema metadata. A collection maps to the tidy
//! long-format batch (`sid: Int64`, `ts: Int64`, `v: Float64`) that a Polars /
//! DuckDB / pandas consumer expects. Both directions round-trip through the
//! Arrow IPC stream format.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use subms_ts::{TsCollection, TsSeries, TsSeriesMetadata};

use crate::error::TsArrowError;

const MK_SID: &str = "subms.sid";
const MK_NAME: &str = "subms.name";
const TAG_PREFIX: &str = "subms.tag.";
const NAME_PREFIX: &str = "subms.name.";

fn meta_to_map(meta: &TsSeriesMetadata) -> HashMap<String, String> {
    let mut md = HashMap::new();
    md.insert(MK_SID.to_string(), meta.id.to_string());
    md.insert(MK_NAME.to_string(), meta.name.clone());
    for (k, v) in meta.tags.iter() {
        md.insert(format!("{TAG_PREFIX}{k}"), v.to_string());
    }
    md
}

fn map_to_meta(md: &HashMap<String, String>) -> Option<TsSeriesMetadata> {
    let sid: u64 = md.get(MK_SID)?.parse().ok()?;
    let name = md.get(MK_NAME).cloned().unwrap_or_default();
    let mut meta = TsSeriesMetadata::new(sid, name);
    for (k, v) in md {
        if let Some(tag) = k.strip_prefix(TAG_PREFIX) {
            meta = meta.with_tag(tag, v);
        }
    }
    Some(meta)
}

/// Map one series to a two-column `RecordBatch` (`ts`, `v`) carrying its
/// identity in schema metadata.
pub fn series_to_batch(series: &TsSeries<f64>) -> Result<RecordBatch, TsArrowError> {
    let ts: Int64Array = series.iter().map(|p| p.ts).collect();
    let v: Float64Array = series.iter().map(|p| p.value).collect();
    let md = series.metadata().map(meta_to_map).unwrap_or_default();
    let schema = Schema::new_with_metadata(
        vec![
            Field::new("ts", DataType::Int64, false),
            Field::new("v", DataType::Float64, false),
        ],
        md,
    );
    RecordBatch::try_new(Arc::new(schema), vec![Arc::new(ts), Arc::new(v)])
        .map_err(|e| TsArrowError::arrow(e.to_string()))
}

fn col_i64<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int64Array, TsArrowError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TsArrowError::mapping(format!("batch missing column {name}")))?
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| TsArrowError::mapping(format!("column {name} is not Int64")))
}

fn col_f64<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Float64Array, TsArrowError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| TsArrowError::mapping(format!("batch missing column {name}")))?
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| TsArrowError::mapping(format!("column {name} is not Float64")))
}

/// Rebuild a series from a two-column batch. Points are sorted by ts before they
/// are pushed, since `TsSeries::push` enforces a monotonic axis.
pub fn batch_to_series(batch: &RecordBatch) -> Result<TsSeries<f64>, TsArrowError> {
    let ts = col_i64(batch, "ts")?;
    let v = col_f64(batch, "v")?;
    let mut pairs: Vec<(i64, f64)> = (0..batch.num_rows())
        .map(|i| (ts.value(i), v.value(i)))
        .collect();
    pairs.sort_by_key(|(t, _)| *t);
    let mut series = TsSeries::<f64>::new();
    if let Some(meta) = map_to_meta(batch.schema().metadata()) {
        series = series.with_metadata(meta);
    }
    for (t, val) in pairs {
        series
            .push(t, val)
            .map_err(|e| TsArrowError::mapping(format!("push failed: {e:?}")))?;
    }
    Ok(series)
}

/// Map a collection to the tidy long-format batch (`sid`, `ts`, `v`). Series
/// names are carried in schema metadata so they survive the round trip.
pub fn collection_to_batch(coll: &TsCollection<f64>) -> Result<RecordBatch, TsArrowError> {
    let mut sids = Vec::new();
    let mut tss = Vec::new();
    let mut vs = Vec::new();
    let mut md = HashMap::new();
    for series in coll.series() {
        let sid = series.metadata().map(|m| m.id).unwrap_or(0);
        if let Some(m) = series.metadata() {
            md.insert(format!("{NAME_PREFIX}{sid}"), m.name.clone());
        }
        for p in series.iter() {
            sids.push(sid as i64);
            tss.push(p.ts);
            vs.push(p.value);
        }
    }
    let schema = Schema::new_with_metadata(
        vec![
            Field::new("sid", DataType::Int64, false),
            Field::new("ts", DataType::Int64, false),
            Field::new("v", DataType::Float64, false),
        ],
        md,
    );
    RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(Int64Array::from(sids)),
            Arc::new(Int64Array::from(tss)),
            Arc::new(Float64Array::from(vs)),
        ],
    )
    .map_err(|e| TsArrowError::arrow(e.to_string()))
}

/// Rebuild a collection from a long-format batch.
pub fn batch_to_collection(batch: &RecordBatch) -> Result<TsCollection<f64>, TsArrowError> {
    let sid = col_i64(batch, "sid")?;
    let ts = col_i64(batch, "ts")?;
    let v = col_f64(batch, "v")?;
    let names = batch.schema();
    let names = names.metadata();

    let mut rows: Vec<(u64, i64, f64)> = (0..batch.num_rows())
        .map(|i| (sid.value(i) as u64, ts.value(i), v.value(i)))
        .collect();
    rows.sort_by_key(|(s, t, _)| (*s, *t));

    let mut coll = TsCollection::<f64>::new();
    let mut current: Option<u64> = None;
    for (s, t, val) in rows {
        if current != Some(s) {
            let name = names
                .get(&format!("{NAME_PREFIX}{s}"))
                .cloned()
                .unwrap_or_default();
            coll.register(TsSeriesMetadata::new(s, name))
                .map_err(|e| TsArrowError::mapping(format!("register failed: {e:?}")))?;
            current = Some(s);
        }
        coll.push(s, t, val)
            .map_err(|e| TsArrowError::mapping(format!("push failed: {e:?}")))?;
    }
    Ok(coll)
}

/// Serialise a batch to an Arrow IPC stream.
pub fn write_ipc(batch: &RecordBatch) -> Result<Vec<u8>, TsArrowError> {
    let mut buf = Vec::new();
    {
        let mut w = StreamWriter::try_new(&mut buf, &batch.schema())
            .map_err(|e| TsArrowError::ipc(e.to_string()))?;
        w.write(batch)
            .map_err(|e| TsArrowError::ipc(e.to_string()))?;
        w.finish().map_err(|e| TsArrowError::ipc(e.to_string()))?;
    }
    Ok(buf)
}

/// Read the first batch from an Arrow IPC stream.
pub fn read_ipc(bytes: &[u8]) -> Result<RecordBatch, TsArrowError> {
    let mut reader =
        StreamReader::try_new(bytes, None).map_err(|e| TsArrowError::ipc(e.to_string()))?;
    match reader.next() {
        Some(b) => b.map_err(|e| TsArrowError::ipc(e.to_string())),
        None => Err(TsArrowError::ipc("ipc stream held no record batch")),
    }
}
