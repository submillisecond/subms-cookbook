use subms_ts::{TsCodec, TsSeries};
use subms_ts_cbor::{TsCborCodec, TsCborError};

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

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// Pins the cross-language wire layout: points (1,1.0) (2,2.0). The Java port
// asserts the identical hex.
const CBOR_FIXTURE: &str = "a2627473820102617682fb3ff0000000000000fb4000000000000000";

#[test]
fn encode_matches_fixture() {
    let s = series(&[(1, 1.0), (2, 2.0)]);
    assert_eq!(to_hex(&TsCborCodec::new().encode(&s)), CBOR_FIXTURE);
}

#[test]
fn round_trip_basic() {
    let s = series(&[(1, 1.5), (2, 2.5), (3, 3.5)]);
    let codec = TsCborCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}

#[test]
fn round_trip_empty() {
    let s = TsSeries::<f64>::new();
    let codec = TsCborCodec::new();
    let bytes = codec.encode(&s);
    let back = codec.decode(&bytes).unwrap();
    assert!(back.is_empty());
}

#[test]
fn round_trip_negative_and_large_ts() {
    let s = series(&[(-1_000_000, -2.5), (0, 0.0), (9_000_000_000, 42.25)]);
    let codec = TsCborCodec::new();
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
    let codec = TsCborCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pts);
}

#[test]
fn values_are_bit_exact() {
    // tricky f64s: round-trip via raw bits, not just approx equality.
    let s = series(&[(1, std::f64::consts::PI), (2, 1.0 / 3.0), (3, 1e-300)]);
    let codec = TsCborCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    for (a, b) in pairs(&s).iter().zip(pairs(&back)) {
        assert_eq!(a.1.to_bits(), b.1.to_bits());
    }
}

#[test]
fn decode_is_key_order_independent() {
    // Hand-build a map with "v" before "ts"; decode must still work.
    let mut buf = vec![0xa2];
    // "v" -> array(1) -> f64 1.0
    buf.extend_from_slice(&[0x61, b'v', 0x81, 0xfb]);
    buf.extend_from_slice(&1.0f64.to_bits().to_be_bytes());
    // "ts" -> array(1) -> int 7
    buf.extend_from_slice(&[0x62, b't', b's', 0x81, 0x07]);
    let back = TsCborCodec::new().decode(&buf).unwrap();
    assert_eq!(pairs(&back), vec![(7, 1.0)]);
}

#[test]
fn format_is_cbor() {
    assert_eq!(TsCborCodec::new().format(), "cbor");
}

#[test]
fn decode_rejects_truncated() {
    let s = series(&[(1, 1.0), (2, 2.0)]);
    let full = TsCborCodec::new().encode(&s);
    let err = TsCborCodec::new().decode(&full[..full.len() - 3]).unwrap_err();
    assert_eq!(err, TsCborError::Truncated);
    assert_eq!(TsCborCodec::new().decode(&[]).unwrap_err(), TsCborError::Truncated);
}

#[test]
fn decode_rejects_non_map() {
    // 0x82 = array(2), not a map.
    let err = TsCborCodec::new().decode(&[0x82, 0x01, 0x02]).unwrap_err();
    assert!(matches!(err, TsCborError::Unexpected(_)));
}

#[test]
fn decode_rejects_length_mismatch() {
    // map(2): ts -> array(2) [1,2], v -> array(1) [1.0]
    let mut buf = vec![0xa2, 0x62, b't', b's', 0x82, 0x01, 0x02, 0x61, b'v', 0x81, 0xfb];
    buf.extend_from_slice(&1.0f64.to_bits().to_be_bytes());
    let err = TsCborCodec::new().decode(&buf).unwrap_err();
    assert!(matches!(err, TsCborError::Unexpected(_)));
}

#[test]
fn decode_rejects_unknown_key() {
    // map(2) with an unexpected key "x".
    let buf = vec![0xa2, 0x61, b'x', 0x80, 0x61, b'v', 0x80];
    let err = TsCborCodec::new().decode(&buf).unwrap_err();
    assert!(matches!(err, TsCborError::Unexpected(_)));
}

#[test]
fn wide_integer_heads_round_trip() {
    // ts values that exercise every int head width (1/2/4/8 byte).
    let s = series(&[(10, 1.0), (300, 2.0), (70_000, 3.0), (5_000_000_000, 4.0)]);
    let codec = TsCborCodec::new();
    let back = codec.decode(&codec.encode(&s)).unwrap();
    assert_eq!(pairs(&back), pairs(&s));
}
