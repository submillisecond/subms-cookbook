use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use subms_ts::{TsCollection, TsSeries, TsSeriesMetadata};
use subms_ts_arrow::{
    TsArrowError, batch_to_collection, batch_to_series, collection_to_batch, read_ipc,
    series_to_batch, write_ipc,
};

fn series(id: u64, name: &str, n: usize) -> TsSeries<f64> {
    let mut s = TsSeries::<f64>::new();
    let base = 1_780_000_000_000_000_000i64;
    for i in 0..n {
        s.push(base + i as i64 * 1_000_000_000, i as f64 * 0.5)
            .unwrap();
    }
    s.with_metadata(
        TsSeriesMetadata::new(id, name)
            .with_tag("host", "edge-01")
            .with_tag("region", "us-east-1"),
    )
}

#[test]
fn series_to_batch_shape() {
    let b = series_to_batch(&series(1, "cpu", 8)).unwrap();
    assert_eq!(b.num_rows(), 8);
    assert_eq!(b.num_columns(), 2);
    assert_eq!(b.schema().field(0).name(), "ts");
    assert_eq!(b.schema().field(1).name(), "v");
}

#[test]
fn series_round_trips_through_batch() {
    let s = series(3, "cpu", 16);
    let back = batch_to_series(&series_to_batch(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 16);
    assert_eq!(back.metadata().unwrap().name, "cpu");
    assert_eq!(back.last().map(|p| p.value), s.last().map(|p| p.value));
}

#[test]
fn batch_preserves_schema_metadata() {
    let b = series_to_batch(&series(9, "trades.aapl", 4)).unwrap();
    let md = b.schema();
    let md = md.metadata();
    assert_eq!(md.get("subms.sid").map(String::as_str), Some("9"));
    assert_eq!(
        md.get("subms.name").map(String::as_str),
        Some("trades.aapl")
    );
    assert_eq!(
        md.get("subms.tag.host").map(String::as_str),
        Some("edge-01")
    );
}

#[test]
fn batch_to_series_sorts_unordered_rows() {
    let schema = Schema::new(vec![
        Field::new("ts", DataType::Int64, false),
        Field::new("v", DataType::Float64, false),
    ]);
    let b = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(Int64Array::from(vec![300i64, 100, 200])),
            Arc::new(Float64Array::from(vec![3.0, 1.0, 2.0])),
        ],
    )
    .unwrap();
    let s = batch_to_series(&b).unwrap();
    let got: Vec<i64> = s.iter().map(|p| p.ts).collect();
    assert_eq!(got, vec![100, 200, 300]);
}

#[test]
fn series_with_no_metadata_round_trips() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 1.0).unwrap();
    s.push(20, 2.0).unwrap();
    let back = batch_to_series(&series_to_batch(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 2);
    assert!(back.metadata().is_none());
}

#[test]
fn missing_column_is_mapping_error() {
    let schema = Schema::new(vec![Field::new("ts", DataType::Int64, false)]);
    let b = RecordBatch::try_new(
        Arc::new(schema),
        vec![Arc::new(Int64Array::from(vec![1i64]))],
    )
    .unwrap();
    assert!(matches!(
        batch_to_series(&b),
        Err(TsArrowError::Mapping { .. })
    ));
}

#[test]
fn wrong_column_type_is_mapping_error() {
    let schema = Schema::new(vec![
        Field::new("ts", DataType::Utf8, false),
        Field::new("v", DataType::Float64, false),
    ]);
    let b = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(StringArray::from(vec!["x"])),
            Arc::new(Float64Array::from(vec![1.0])),
        ],
    )
    .unwrap();
    assert!(matches!(
        batch_to_series(&b),
        Err(TsArrowError::Mapping { .. })
    ));
}

#[test]
fn collection_round_trips_through_long_batch() {
    let mut coll = TsCollection::<f64>::new();
    for (id, name) in [(1u64, "a"), (2, "b"), (3, "c")] {
        coll.register(TsSeriesMetadata::new(id, name)).unwrap();
        for i in 0..5 {
            coll.push(id, 1_000 + i, i as f64).unwrap();
        }
    }
    let batch = collection_to_batch(&coll).unwrap();
    assert_eq!(batch.num_rows(), 15);
    assert_eq!(batch.num_columns(), 3);
    let back = batch_to_collection(&batch).unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back.by_name("b").map(|s| s.len()), Some(5));
}

#[test]
fn empty_collection_yields_empty_batch() {
    let coll = TsCollection::<f64>::new();
    let batch = collection_to_batch(&coll).unwrap();
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch_to_collection(&batch).unwrap().len(), 0);
}

#[test]
fn ipc_round_trips_a_series_batch() {
    let s = series(5, "cpu", 32);
    let ipc = write_ipc(&series_to_batch(&s).unwrap()).unwrap();
    let back = batch_to_series(&read_ipc(&ipc).unwrap()).unwrap();
    assert_eq!(back.len(), 32);
    assert_eq!(back.metadata().unwrap().name, "cpu");
}

#[test]
fn ipc_round_trips_a_collection_batch() {
    let mut coll = TsCollection::<f64>::new();
    coll.register(TsSeriesMetadata::new(7, "x")).unwrap();
    for i in 0..10 {
        coll.push(7, 1_000 + i, i as f64).unwrap();
    }
    let ipc = write_ipc(&collection_to_batch(&coll).unwrap()).unwrap();
    let back = batch_to_collection(&read_ipc(&ipc).unwrap()).unwrap();
    assert_eq!(back.by_name("x").map(|s| s.len()), Some(10));
}

#[test]
fn read_ipc_rejects_garbage() {
    assert!(matches!(
        read_ipc(&[1, 2, 3, 4]),
        Err(TsArrowError::Ipc { .. })
    ));
}

#[test]
fn empty_ipc_stream_is_error() {
    // a valid stream of a zero-row batch still yields the batch; truly empty bytes error
    assert!(read_ipc(&[]).is_err());
}
