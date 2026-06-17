use subms_ts::{TsCollection, TsSeries, TsSeriesMetadata};
use subms_ts_parquet::{
    TsParquetError, collection_to_parquet, parquet_to_collection, parquet_to_series,
    series_to_parquet,
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
fn parquet_bytes_have_magic_header() {
    let bytes = series_to_parquet(&series(1, "cpu", 8)).unwrap();
    assert_eq!(&bytes[0..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
}

#[test]
fn series_round_trips_through_parquet() {
    let s = series(3, "cpu", 64);
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 64);
    assert_eq!(back.metadata().unwrap().name, "cpu");
    assert_eq!(back.last().map(|p| p.value), s.last().map(|p| p.value));
}

#[test]
fn parquet_preserves_metadata_and_tags() {
    let s = series(9, "trades.aapl", 4);
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    let m = back.metadata().unwrap();
    assert_eq!(m.id, 9);
    assert_eq!(m.name, "trades.aapl");
    assert_eq!(m.tags.get("host").map(String::as_str), Some("edge-01"));
    assert_eq!(m.tags.get("region").map(String::as_str), Some("us-east-1"));
}

#[test]
fn series_values_survive_exactly() {
    let mut s = TsSeries::<f64>::new();
    for (t, v) in [(1i64, 1.25f64), (2, -3.5), (3, 1e300), (4, 0.0)] {
        s.push(t, v).unwrap();
    }
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    let got: Vec<(i64, f64)> = back.iter().map(|p| (p.ts, p.value)).collect();
    assert_eq!(got, vec![(1, 1.25), (2, -3.5), (3, 1e300), (4, 0.0)]);
}

#[test]
fn empty_series_round_trips() {
    let s = TsSeries::<f64>::new().with_metadata(TsSeriesMetadata::new(1, "empty"));
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 0);
    assert_eq!(back.metadata().unwrap().name, "empty");
}

#[test]
fn series_with_no_metadata_round_trips() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 1.0).unwrap();
    s.push(20, 2.0).unwrap();
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 2);
}

#[test]
fn collection_round_trips_through_parquet() {
    let mut coll = TsCollection::<f64>::new();
    for (id, name) in [(1u64, "a"), (2, "b"), (3, "c")] {
        coll.register(TsSeriesMetadata::new(id, name)).unwrap();
        for i in 0..5 {
            coll.push(id, 1_000 + i, i as f64).unwrap();
        }
    }
    let back = parquet_to_collection(&collection_to_parquet(&coll).unwrap()).unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back.by_name("b").map(|s| s.len()), Some(5));
}

#[test]
fn empty_collection_round_trips() {
    let coll = TsCollection::<f64>::new();
    let back = parquet_to_collection(&collection_to_parquet(&coll).unwrap()).unwrap();
    assert_eq!(back.len(), 0);
}

#[test]
fn decode_rejects_garbage() {
    assert!(matches!(
        parquet_to_series(&[1, 2, 3, 4, 5, 6, 7, 8]),
        Err(TsParquetError::Parquet { .. })
    ));
}

#[test]
fn decode_rejects_empty_bytes() {
    assert!(parquet_to_series(&[]).is_err());
}

#[test]
fn larger_series_round_trips() {
    let s = series(1, "cpu", 5_000);
    let back = parquet_to_series(&series_to_parquet(&s).unwrap()).unwrap();
    assert_eq!(back.len(), 5_000);
    assert_eq!(back.last().map(|p| p.value), s.last().map(|p| p.value));
}

#[test]
fn parquet_is_smaller_than_naive_for_repetitive_data() {
    // a constant-value series compresses well under dictionary / RLE encoding
    let mut s = TsSeries::<f64>::new();
    for i in 0..2_000 {
        s.push(i, 42.0).unwrap();
    }
    let bytes = series_to_parquet(&s).unwrap();
    // 2000 points * 16 bytes raw = 32000; parquet must beat that on constant v
    assert!(
        bytes.len() < 32_000,
        "parquet len {} not < 32000",
        bytes.len()
    );
}
