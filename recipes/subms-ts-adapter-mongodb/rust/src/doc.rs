//! The pure mapping core: `TsSeries<f64>` / metadata <-> BSON documents.
//!
//! Point documents follow the canonical MongoDB time-series shape
//! `{ _id: { sid: <i64>, ts: <i64> }, v: <f64> }`, so a range scan over one
//! series rides the `(_id.sid, _id.ts)` compound index as a single B-tree walk.
//! Series identity (name, tags, schema) lives in a sidecar metadata document
//! keyed by the numeric series id.

use bson::{Bson, Document, doc};
use subms_ts::{TsNumericKind, TsSchema, TsSeries, TsSeriesMetadata};

use crate::error::TsMongoError;

/// The collection that holds one metadata document per series.
pub const META_COLLECTION: &str = "ts_meta";

/// The per-series point collection name (`ts_<sid>`).
pub fn point_collection(series_id: u64) -> String {
    format!("ts_{series_id}")
}

/// Build the per-point document `{ _id: { sid, ts }, v }`.
pub fn point_doc(series_id: u64, ts: i64, value: f64) -> Document {
    doc! {
        "_id": doc! { "sid": series_id as i64, "ts": ts },
        "v": Bson::Double(value),
    }
}

/// Build the sidecar metadata document for a series.
pub fn meta_doc(meta: &TsSeriesMetadata) -> Document {
    let mut tags = Document::new();
    for (k, v) in meta.tags.iter() {
        tags.insert(k.clone(), v.clone());
    }
    let mut d = doc! {
        "_id": meta.id as i64,
        "name": meta.name.clone(),
        "tags": tags,
    };
    if let Some(schema) = schema_doc(&meta.schema) {
        d.insert("schema", schema);
    }
    d
}

fn schema_doc(schema: &TsSchema) -> Option<Document> {
    match schema {
        TsSchema::Numeric { unit, kind } => {
            let mut s = doc! { "kind": numeric_kind_str(*kind) };
            if let Some(u) = unit {
                s.insert("unit", u.clone());
            }
            Some(s)
        }
        TsSchema::Custom { type_name } => {
            Some(doc! { "kind": "custom", "type_name": type_name.clone() })
        }
        TsSchema::Schemaless => Some(doc! { "kind": "schemaless" }),
        TsSchema::Anonymous => None,
    }
}

fn numeric_kind_str(kind: TsNumericKind) -> &'static str {
    match kind {
        TsNumericKind::Gauge => "gauge",
        TsNumericKind::Counter => "counter",
        TsNumericKind::Rate => "rate",
    }
}

fn parse_schema(d: Option<&Document>) -> TsSchema {
    let Some(d) = d else {
        return TsSchema::Anonymous;
    };
    match d.get_str("kind").unwrap_or("") {
        "gauge" | "counter" | "rate" => {
            let kind = match d.get_str("kind").unwrap_or("gauge") {
                "counter" => TsNumericKind::Counter,
                "rate" => TsNumericKind::Rate,
                _ => TsNumericKind::Gauge,
            };
            let unit = d.get_str("unit").ok().map(str::to_string);
            TsSchema::Numeric { unit, kind }
        }
        "schemaless" => TsSchema::Schemaless,
        "custom" => TsSchema::Custom {
            type_name: d.get_str("type_name").unwrap_or("").to_string(),
        },
        _ => TsSchema::Anonymous,
    }
}

/// Reconstruct `TsSeriesMetadata` from a sidecar document.
pub fn meta_from_doc(d: &Document) -> Result<TsSeriesMetadata, TsMongoError> {
    let id = d
        .get_i64("_id")
        .map_err(|_| TsMongoError::mapping("meta doc missing i64 _id"))? as u64;
    let name = d.get_str("name").unwrap_or("").to_string();
    let mut meta =
        TsSeriesMetadata::new(id, name).with_schema(parse_schema(d.get_document("schema").ok()));
    if let Ok(tags) = d.get_document("tags") {
        for (k, v) in tags.iter() {
            if let Bson::String(v) = v {
                meta = meta.with_tag(k.clone(), v.clone());
            }
        }
    }
    Ok(meta)
}

/// Decode one point document into `(ts, value)`.
pub fn point_from_doc(d: &Document) -> Result<(i64, f64), TsMongoError> {
    let id = d
        .get_document("_id")
        .map_err(|_| TsMongoError::mapping("point doc missing _id sub-document"))?;
    let ts = id
        .get_i64("ts")
        .map_err(|_| TsMongoError::mapping("point _id missing i64 ts"))?;
    let v = d
        .get_f64("v")
        .map_err(|_| TsMongoError::mapping("point doc missing f64 v"))?;
    Ok((ts, v))
}

/// Turn a series into its `(metadata document, point documents)` pair. A series
/// with no metadata is mapped under series id 0.
pub fn series_to_docs(series: &TsSeries<f64>) -> (Document, Vec<Document>) {
    let meta = series
        .metadata()
        .cloned()
        .unwrap_or_else(|| TsSeriesMetadata::new(0, ""));
    let sid = meta.id;
    let points = series
        .iter()
        .map(|p| point_doc(sid, p.ts, p.value))
        .collect();
    (meta_doc(&meta), points)
}

/// Rebuild a series from its metadata document and an unordered slice of point
/// documents. Points are sorted by ts before they are pushed, because
/// `TsSeries::push` enforces a monotonic time axis and a query result is not
/// guaranteed to arrive ordered.
pub fn series_from_docs(
    meta: &Document,
    points: &[Document],
) -> Result<TsSeries<f64>, TsMongoError> {
    let meta = meta_from_doc(meta)?;
    let mut decoded: Vec<(i64, f64)> = points
        .iter()
        .map(point_from_doc)
        .collect::<Result<_, _>>()?;
    decoded.sort_by_key(|(ts, _)| *ts);
    let mut series = TsSeries::<f64>::new().with_metadata(meta);
    for (ts, v) in decoded {
        series
            .push(ts, v)
            .map_err(|e| TsMongoError::mapping(format!("push failed: {e:?}")))?;
    }
    Ok(series)
}

/// Encode a document to its canonical BSON byte form (the on-the-wire shape).
pub fn doc_to_bytes(d: &Document) -> Result<Vec<u8>, TsMongoError> {
    bson::to_vec(d).map_err(|e| TsMongoError::bson(e.to_string()))
}

/// Decode BSON bytes back into a document.
pub fn doc_from_bytes(bytes: &[u8]) -> Result<Document, TsMongoError> {
    bson::from_slice(bytes).map_err(|e| TsMongoError::bson(e.to_string()))
}
