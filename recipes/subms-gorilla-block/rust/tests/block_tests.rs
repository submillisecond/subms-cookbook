use subms_gorilla_block::{TsBlockError, TsGorillaBlock};

fn roundtrip(points: &[(i64, f64)]) -> Vec<(i64, f64)> {
    let mut b = TsGorillaBlock::new();
    for &(t, v) in points {
        b.append(t, v);
    }
    let bytes = b.bytes();
    let decoded = TsGorillaBlock::from_bytes(&bytes).unwrap();
    decoded.iter().map(|p| (p.ts, p.value)).collect()
}

#[test]
fn empty_block() {
    let b = TsGorillaBlock::new();
    assert!(b.is_empty());
    assert_eq!(b.iter().count(), 0);
    let bytes = b.bytes();
    assert_eq!(TsGorillaBlock::from_bytes(&bytes).unwrap().len(), 0);
}

#[test]
fn single_point() {
    assert_eq!(
        roundtrip(&[(1_700_000_000_000, 123.456)]),
        vec![(1_700_000_000_000, 123.456)]
    );
}

#[test]
fn constant_value_xor_zero_path() {
    let pts: Vec<(i64, f64)> = (0..500).map(|i| (i, 42.0)).collect();
    assert_eq!(roundtrip(&pts), pts);
}

#[test]
fn constant_interval_dod_zero_path() {
    let pts: Vec<(i64, f64)> = (0..500).map(|i| (i * 1_000, i as f64 * 0.5)).collect();
    assert_eq!(roundtrip(&pts), pts);
}

#[test]
fn irregular_intervals_and_large_jumps() {
    let pts = vec![
        (0i64, 1.0),
        (5, 1.5),
        (5_000_000, 2.0),       // big jump -> 64-bit dod fallback
        (5_000_063, 2.0),       // small dod
        (10_000_000_000, -7.5), // huge jump
    ];
    assert_eq!(roundtrip(&pts), pts);
}

#[test]
fn negative_and_mixed_values() {
    let pts = vec![
        (1, -1.0),
        (2, -2.5),
        (3, 0.0),
        (4, 1e-9),
        (5, -1e9),
        (6, 123456.789),
    ];
    assert_eq!(roundtrip(&pts), pts);
}

#[test]
fn random_walk_bit_exact() {
    // deterministic LCG walk - exact f64 round-trip (Gorilla is lossless)
    let mut state = 88172645463325252u64;
    let mut v = 100.0f64;
    let mut pts = Vec::new();
    for i in 0..2_000i64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        v += ((state >> 40) as f64 / u32::MAX as f64) - 0.5;
        pts.push((i * 1_000 + (state & 7) as i64, v));
    }
    assert_eq!(roundtrip(&pts), pts);
}

#[test]
fn reencode_is_deterministic() {
    let pts: Vec<(i64, f64)> = (0..1_000)
        .map(|i| (i * 1_000, (i as f64 * 0.01).sin()))
        .collect();
    let mut b = TsGorillaBlock::new();
    for &(t, v) in &pts {
        b.append(t, v);
    }
    let bytes1 = b.bytes();
    // from_bytes decodes + re-encodes; the wire must be byte-identical.
    let bytes2 = TsGorillaBlock::from_bytes(&bytes1).unwrap().bytes();
    assert_eq!(bytes1, bytes2);
}

#[test]
fn compresses_constant_value_hard() {
    // Constant value (xor=0) + 1/sec ticks (dod=0): ~1 bit/point each.
    let mut b = TsGorillaBlock::new();
    for i in 0..4_096i64 {
        b.append(1_700_000_000 + i, 42.0);
    }
    let per_point = b.bytes().len() as f64 / 4_096.0;
    assert!(
        per_point < 1.0,
        "constant series: {per_point:.3} bytes/point"
    );
}

#[test]
fn compresses_stepped_gauge() {
    // A realistic gauge that holds steady then steps - the common metric
    // shape. Most points hit the xor=0 path. Beats raw 16 bytes/point well.
    let mut b = TsGorillaBlock::new();
    for i in 0..4_096i64 {
        let v = 20.0 + (i / 16) as f64; // changes once every 16 ticks
        b.append(1_700_000_000 + i, v);
    }
    let per_point = b.bytes().len() as f64 / 4_096.0;
    assert!(per_point < 4.0, "stepped gauge: {per_point:.3} bytes/point");
}

#[test]
fn range_filter() {
    let mut b = TsGorillaBlock::new();
    for i in 0..100i64 {
        b.append(i, i as f64);
    }
    let got: Vec<i64> = b.range(40, 45).map(|p| p.ts).collect();
    assert_eq!(got, vec![40, 41, 42, 43, 44, 45]);
    assert_eq!(b.range(200, 300).count(), 0);
}

#[test]
fn merge_orders_points() {
    let mut a = TsGorillaBlock::new();
    for i in 0..50i64 {
        a.append(i, i as f64);
    }
    let mut c = TsGorillaBlock::new();
    for i in 50..100i64 {
        c.append(i, i as f64);
    }
    let m = a.merge(&c);
    assert_eq!(m.len(), 100);
    let ts: Vec<i64> = m.iter().map(|p| p.ts).collect();
    assert_eq!(ts, (0..100).collect::<Vec<_>>());
}

#[test]
fn stats_track_extremes() {
    let mut b = TsGorillaBlock::new();
    for &(t, v) in &[(10i64, 5.0), (20, 1.0), (30, 9.0), (40, 3.0)] {
        b.append(t, v);
    }
    let s = b.stats();
    assert_eq!(s.count, 4);
    assert_eq!(s.ts_min, 10);
    assert_eq!(s.ts_max, 40);
    assert_eq!(s.value_min, 1.0);
    assert_eq!(s.value_max, 9.0);
}

#[test]
fn bad_version_rejected() {
    let mut bytes = TsGorillaBlock::new();
    bytes.append(1, 1.0);
    let mut raw = bytes.bytes();
    raw[0] = 99;
    assert!(matches!(
        TsGorillaBlock::from_bytes(&raw),
        Err(TsBlockError::BadVersion(99))
    ));
}
