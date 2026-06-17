use bson::{Bson, doc};
use subms_ts::{TsNumericKind, TsSchema, TsSeries, TsSeriesMetadata};
use subms_ts_mongodb::{
    InMemoryMongoStore, META_COLLECTION, TsChangeEvent, TsMongoAdapter, TsMongoError, TsMongoStore,
    doc_from_bytes, doc_to_bytes, meta_doc, meta_from_doc, point_collection, point_doc,
    point_from_doc, series_from_docs, series_to_docs,
};

fn tagged(id: u64, name: &str) -> TsSeriesMetadata {
    TsSeriesMetadata::new(id, name)
        .with_schema(TsSchema::Numeric {
            unit: Some("ms".into()),
            kind: TsNumericKind::Gauge,
        })
        .with_tag("host", "edge-01")
        .with_tag("region", "us-east-1")
}

fn series(id: u64, name: &str, n: usize) -> TsSeries<f64> {
    let mut s = TsSeries::<f64>::new();
    let base = 1_780_000_000_000_000_000i64;
    for i in 0..n {
        s.push(base + i as i64 * 1_000_000_000, i as f64 * 0.5)
            .unwrap();
    }
    s.with_metadata(tagged(id, name))
}

#[test]
fn point_doc_has_canonical_shape() {
    let d = point_doc(7, 1_780_000_000_000_000_000, 0.42);
    let id = d.get_document("_id").unwrap();
    assert_eq!(id.get_i64("sid").unwrap(), 7);
    assert_eq!(id.get_i64("ts").unwrap(), 1_780_000_000_000_000_000);
    assert_eq!(d.get_f64("v").unwrap(), 0.42);
}

#[test]
fn point_collection_name() {
    assert_eq!(point_collection(42), "ts_42");
}

#[test]
fn point_round_trips_through_doc() {
    let d = point_doc(1, 100, 3.5);
    let (ts, v) = point_from_doc(&d).unwrap();
    assert_eq!(ts, 100);
    assert_eq!(v, 3.5);
}

#[test]
fn point_round_trips_through_bson_bytes() {
    let d = point_doc(1, 100, 3.5);
    let bytes = doc_to_bytes(&d).unwrap();
    let back = doc_from_bytes(&bytes).unwrap();
    assert_eq!(d, back);
}

// Pins the exact BSON byte layout of a canonical point document. The Java port
// asserts the identical hex, so a Rust-encoded document decodes unchanged in a
// Java query node and vice versa.
#[test]
fn point_bson_matches_cross_language_fixture() {
    let bytes = doc_to_bytes(&point_doc(1, 100, 3.5)).unwrap();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "33000000035f6964001e00000012736964000100000000000000127473006400000000000000000176000000000000000c4000"
    );
}

#[test]
fn meta_doc_preserves_identity() {
    let m = tagged(9, "trades.aapl");
    let d = meta_doc(&m);
    let back = meta_from_doc(&d).unwrap();
    assert_eq!(back.id, 9);
    assert_eq!(back.name, "trades.aapl");
    assert_eq!(back.tags.get("host").map(String::as_str), Some("edge-01"));
    assert_eq!(
        back.tags.get("region").map(String::as_str),
        Some("us-east-1")
    );
    match back.schema {
        TsSchema::Numeric { unit, kind } => {
            assert_eq!(unit.as_deref(), Some("ms"));
            assert_eq!(kind, TsNumericKind::Gauge);
        }
        other => panic!("expected numeric schema, got {other:?}"),
    }
}

#[test]
fn meta_doc_anonymous_schema_round_trips() {
    let m = TsSeriesMetadata::new(1, "x");
    let back = meta_from_doc(&meta_doc(&m)).unwrap();
    assert!(matches!(back.schema, TsSchema::Anonymous));
}

#[test]
fn series_round_trips_through_docs() {
    let s = series(3, "cpu", 16);
    let (meta, points) = series_to_docs(&s);
    assert_eq!(points.len(), 16);
    let back = series_from_docs(&meta, &points).unwrap();
    assert_eq!(back.len(), 16);
    assert_eq!(back.metadata().unwrap().name, "cpu");
    assert_eq!(back.last().map(|p| p.value), s.last().map(|p| p.value));
}

#[test]
fn series_from_docs_sorts_unordered_points() {
    let meta = meta_doc(&tagged(1, "cpu"));
    let points = vec![
        point_doc(1, 300, 3.0),
        point_doc(1, 100, 1.0),
        point_doc(1, 200, 2.0),
    ];
    let s = series_from_docs(&meta, &points).unwrap();
    let got: Vec<i64> = s.iter().map(|p| p.ts).collect();
    assert_eq!(got, vec![100, 200, 300]);
}

#[test]
fn series_with_no_metadata_maps_to_id_zero() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 1.0).unwrap();
    let (meta, _) = series_to_docs(&s);
    assert_eq!(meta.get_i64("_id").unwrap(), 0);
}

#[test]
fn point_from_doc_rejects_malformed() {
    let bad = doc! { "_id": 5i64, "v": 1.0 };
    assert!(matches!(
        point_from_doc(&bad),
        Err(TsMongoError::Mapping { .. })
    ));
}

#[test]
fn adapter_write_then_read_series() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    let s = series(5, "cpu", 32);
    let n = adapter.write_series(&s).unwrap();
    assert_eq!(n, 32);
    let back = adapter.read_series(5).unwrap();
    assert_eq!(back.len(), 32);
    assert_eq!(back.metadata().unwrap().name, "cpu");
}

#[test]
fn adapter_read_unknown_series_errors() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    assert!(matches!(
        adapter.read_series(99),
        Err(TsMongoError::Mapping { .. })
    ));
}

#[test]
fn adapter_write_then_read_collection() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    let mut coll = subms_ts::TsCollection::<f64>::new();
    for (id, name) in [(1u64, "a"), (2, "b"), (3, "c")] {
        let s = series(id, name, 8);
        adapter.write_series(&s).unwrap();
        let _ = &mut coll;
    }
    let back = adapter.read_collection().unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back.by_name("b").map(|s| s.len()), Some(8));
}

#[test]
fn adapter_empty_series_writes_meta_only() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    let s = TsSeries::<f64>::new().with_metadata(tagged(4, "empty"));
    let n = adapter.write_series(&s).unwrap();
    assert_eq!(n, 0);
    assert_eq!(adapter.store().count(META_COLLECTION), 1);
}

#[test]
fn ensure_indexes_creates_compound_index() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    adapter.write_series(&series(5, "cpu", 4)).unwrap();
    adapter.ensure_indexes().unwrap();
    let idx = adapter.store().indexes(&point_collection(5));
    assert_eq!(idx.len(), 1);
    assert_eq!(idx[0], doc! { "_id.sid": 1, "_id.ts": 1 });
    // the metadata collection is not a point collection - no index
    assert!(adapter.store().indexes(META_COLLECTION).is_empty());
}

#[test]
fn change_events_capture_every_insert() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    adapter.write_series(&series(5, "cpu", 4)).unwrap();
    let changes = adapter.poll_changes().unwrap();
    // 1 meta doc + 4 point docs
    assert_eq!(changes.len(), 5);
    assert!(matches!(changes[0], TsChangeEvent::Insert { .. }));
    // draining a second time yields nothing
    assert!(adapter.poll_changes().unwrap().is_empty());
}

#[test]
fn store_find_one_by_id() {
    let store = InMemoryMongoStore::new();
    store
        .insert_many("ts_1", vec![point_doc(1, 100, 1.0), point_doc(1, 200, 2.0)])
        .unwrap();
    let found = store
        .find_one("ts_1", &Bson::Document(doc! { "sid": 1i64, "ts": 200i64 }))
        .unwrap();
    assert_eq!(found.unwrap().get_f64("v").unwrap(), 2.0);
    let missing = store
        .find_one("ts_1", &Bson::Document(doc! { "sid": 1i64, "ts": 999i64 }))
        .unwrap();
    assert!(missing.is_none());
}

#[test]
fn store_collections_lists_written_names() {
    let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
    adapter.write_series(&series(1, "a", 2)).unwrap();
    adapter.write_series(&series(2, "b", 2)).unwrap();
    let mut names = adapter.store().collections().unwrap();
    names.sort();
    assert_eq!(names, vec!["ts_1", "ts_2", "ts_meta"]);
}
