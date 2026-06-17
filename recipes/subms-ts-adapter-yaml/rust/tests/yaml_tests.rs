use saphyr::{LoadableYamlNode, Yaml};
use subms_ts::{TsCodec, TsSeries, TsTimestampStyle};
use subms_ts_yaml::{TsYamlCodec, TsYamlError};

fn series(points: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(t, v) in points {
        s.push(t, v).unwrap();
    }
    s
}

fn pairs(s: &TsSeries<f64>) -> Vec<(i64, f64)> {
    s.iter().map(|p| (p.ts, p.value)).collect()
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn round_trip_basic() {
    let s = series(&[(1, 1.5), (2, 2.5), (3, 3.5)]);
    let codec = TsYamlCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_empty() {
    let s = TsSeries::<f64>::new();
    let codec = TsYamlCodec::new();
    let bytes = codec.encode(&s);
    let back = codec.decode(&bytes).unwrap();
    assert!(back.is_empty());
}

#[test]
fn round_trip_negative_and_large_ts() {
    let s = series(&[(-1_000_000, -2.5), (0, 0.0), (9_000_000_000, 42.25)]);
    let codec = TsYamlCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_many_points() {
    let mut state: u64 = 0xabcd_1234;
    let pts: Vec<(i64, f64)> = (0..5_000)
        .map(|i| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (i, (state >> 11) as f64 / 13.0 - 100.0)
        })
        .collect();
    let s = series(&pts);
    let codec = TsYamlCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pts);
}

#[test]
fn values_round_trip_bit_exact() {
    // The shortest-round-trippable formatting is exact for these; we assert by
    // bit pattern, not approximate equality.
    let s = series(&[(1, std::f64::consts::PI), (2, 1.0 / 3.0), (3, 1e-300)]);
    let codec = TsYamlCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    for (a, b) in pairs(&s).iter().zip(pairs(&back)) {
        assert_eq!(a.1.to_bits(), b.1.to_bits());
    }
}

#[test]
fn encode_is_valid_yaml() {
    // Re-parse the emitted document with the library to confirm it is
    // well-formed YAML, independent of our own decode path.
    let s = series(&[(10, 1.5), (20, 2.5)]);
    let bytes = TsYamlCodec::new().encode(&s);
    let docs = Yaml::load_from_str(text(&bytes)).unwrap();
    let root = docs[0].as_mapping_get("subms_ts_series").unwrap();
    assert_eq!(root.as_mapping_get("timestamps").unwrap().as_vec().unwrap().len(), 2);
    assert_eq!(root.as_mapping_get("values").unwrap().as_vec().unwrap().len(), 2);
}

#[test]
fn encode_is_columnar_block_layout() {
    let s = series(&[(1, 1.0), (2, 2.0)]);
    let doc = String::from_utf8(TsYamlCodec::new().encode(&s)).unwrap();
    assert_eq!(
        doc,
        "subms_ts_series:\n  timestamps:\n  - 1\n  - 2\n  values:\n  - 1.0\n  - 2.0\n"
    );
}

#[test]
fn encode_epoch_nanos_is_raw_integer() {
    let s = series(&[(1_500_000_000, 7.0)]);
    let codec = TsYamlCodec::new().with_style(TsTimestampStyle::EpochNanos);
    let doc = String::from_utf8(codec.encode(&s)).unwrap();
    assert!(doc.contains("- 1500000000\n"), "doc: {doc}");
}

#[test]
fn encode_epoch_millis_scales_down() {
    let s = series(&[(1_500_000_000, 7.0)]);
    let codec = TsYamlCodec::new().with_style(TsTimestampStyle::EpochMillis);
    let doc = String::from_utf8(codec.encode(&s)).unwrap();
    assert!(doc.contains("- 1500\n"), "doc: {doc}");
}

#[test]
fn epoch_millis_round_trips_through_nanos() {
    // Encode at millis, decode at millis: the i64-nanos timestamp is recovered
    // (the encode divides by 1e6, the decode multiplies back).
    let s = series(&[(1_500_000_000, 7.0), (3_000_000_000, 9.0)]);
    let codec = TsYamlCodec::new().with_style(TsTimestampStyle::EpochMillis);
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn encode_iso8601_renders_timestamp_strings() {
    let s = series(&[(0, 1.0)]);
    let codec = TsYamlCodec::new().with_style(TsTimestampStyle::Iso8601);
    let doc = String::from_utf8(codec.encode(&s)).unwrap();
    assert!(doc.contains("1970-01-01T00:00:00.000000000Z"), "doc: {doc}");
}

#[test]
fn decode_iso8601_is_unsupported() {
    let s = series(&[(0, 1.0)]);
    let codec = TsYamlCodec::new().with_style(TsTimestampStyle::Iso8601);
    let bytes = codec.encode(&s);
    let err = codec.decode(&bytes).unwrap_err();
    assert_eq!(err, TsYamlError::UnsupportedTimestampDecode);
}

#[test]
fn decode_accepts_whole_number_values_as_integers() {
    // A hand-edited document may write `3` instead of `3.0`; decode coerces.
    let doc = "subms_ts_series:\n  timestamps:\n  - 1\n  - 2\n  values:\n  - 3\n  - 4\n";
    let back = TsYamlCodec::new().decode(doc.as_bytes()).unwrap();
    assert_eq!(pairs(&back), vec![(1, 3.0), (2, 4.0)]);
}

#[test]
fn decode_accepts_flow_sequences() {
    // The library parses flow style too, even though we emit block style.
    let doc = "subms_ts_series:\n  timestamps: [1, 2, 3]\n  values: [1.5, 2.5, 3.5]\n";
    let back = TsYamlCodec::new().decode(doc.as_bytes()).unwrap();
    assert_eq!(pairs(&back), vec![(1, 1.5), (2, 2.5), (3, 3.5)]);
}

#[test]
fn decode_rejects_malformed_yaml() {
    // Unbalanced flow bracket - a syntax error the parser catches.
    let doc = "subms_ts_series:\n  timestamps: [1, 2\n  values: [1.0]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)), "got {err:?}");
}

#[test]
fn decode_rejects_missing_root() {
    let doc = "other_key:\n  timestamps: [1]\n  values: [1.0]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)));
}

#[test]
fn decode_rejects_missing_values_column() {
    let doc = "subms_ts_series:\n  timestamps: [1, 2]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)));
}

#[test]
fn decode_rejects_length_mismatch() {
    let doc = "subms_ts_series:\n  timestamps: [1, 2, 3]\n  values: [1.0, 2.0]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)));
}

#[test]
fn decode_rejects_non_integer_timestamp() {
    let doc = "subms_ts_series:\n  timestamps: [hello]\n  values: [1.0]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)));
}

#[test]
fn decode_rejects_scalar_instead_of_sequence() {
    let doc = "subms_ts_series:\n  timestamps: 1\n  values: [1.0]\n";
    let err = TsYamlCodec::new().decode(doc.as_bytes()).unwrap_err();
    assert!(matches!(err, TsYamlError::Parse(_)));
}

#[test]
fn format_is_yaml() {
    assert_eq!(TsYamlCodec::new().format(), "yaml");
}
