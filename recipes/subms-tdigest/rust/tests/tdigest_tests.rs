use subms_tdigest::{TsTDigest, TsTDigestError};

fn exact_quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[test]
fn empty_digest() {
    let d = TsTDigest::new(100.0);
    assert!(d.is_empty());
    assert!(d.quantile(0.5).is_nan());
    assert_eq!(d.count(), 0.0);
}

#[test]
fn single_value() {
    let mut d = TsTDigest::new(100.0);
    d.add(42.0);
    assert_eq!(d.quantile(0.0), 42.0);
    assert_eq!(d.quantile(0.5), 42.0);
    assert_eq!(d.quantile(1.0), 42.0);
}

#[test]
fn min_max_exact_at_edges() {
    let mut d = TsTDigest::new(100.0);
    for i in 0..10_000 {
        d.add(i as f64);
    }
    assert_eq!(d.quantile(0.0), 0.0); // min exact
    assert_eq!(d.quantile(1.0), 9_999.0); // max exact
}

#[test]
fn uniform_quantiles_within_bound() {
    // uniform 0..1: cross-check against the exact sorted array.
    let n = 100_000usize;
    let mut vals: Vec<f64> = Vec::with_capacity(n);
    let mut state = 0x2545F4914F6CDD1Du64;
    for _ in 0..n {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        vals.push((state >> 11) as f64 / (1u64 << 53) as f64);
    }
    let mut d = TsTDigest::new(200.0);
    for &v in &vals {
        d.add(v);
    }
    let mut sorted = vals.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    for &q in &[0.01, 0.1, 0.5, 0.9, 0.99, 0.999] {
        let est = d.quantile(q);
        let exact = exact_quantile(&sorted, q);
        assert!(
            (est - exact).abs() < 0.02,
            "q={q}: est={est}, exact={exact}, err={}",
            (est - exact).abs()
        );
    }
}

#[test]
fn tail_is_tighter_than_median() {
    // t-digest's whole point: relative error shrinks toward the tails.
    let n = 100_000usize;
    let mut vals: Vec<f64> = (0..n).map(|i| i as f64 / n as f64).collect();
    let mut d = TsTDigest::new(200.0);
    for &v in &vals {
        d.add(v);
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let err_50 = (d.quantile(0.5) - exact_quantile(&vals, 0.5)).abs();
    let err_999 = (d.quantile(0.999) - exact_quantile(&vals, 0.999)).abs();
    assert!(
        err_999 <= err_50 + 1e-9,
        "tail err {err_999} should be <= median err {err_50}"
    );
}

#[test]
fn cdf_roundtrips_with_quantile() {
    let mut d = TsTDigest::new(200.0);
    for i in 0..10_000 {
        d.add(i as f64);
    }
    // cdf(quantile(q)) ~= q
    for &q in &[0.1, 0.5, 0.9] {
        let v = d.quantile(q);
        let back = d.cdf(v);
        assert!((back - q).abs() < 0.02, "q={q}, cdf(quantile)={back}");
    }
    assert_eq!(d.cdf(-1.0), 0.0);
    assert_eq!(d.cdf(1e9), 1.0);
}

#[test]
fn weighted_add() {
    let mut d = TsTDigest::new(100.0);
    d.add_weighted(10.0, 100.0);
    d.add_weighted(20.0, 100.0);
    assert_eq!(d.count(), 200.0);
    let med = d.quantile(0.5);
    assert!((10.0..=20.0).contains(&med));
}

#[test]
fn merge_matches_combined() {
    let mut a = TsTDigest::new(200.0);
    let mut b = TsTDigest::new(200.0);
    let mut combined = TsTDigest::new(200.0);
    for i in 0..50_000 {
        a.add(i as f64);
        combined.add(i as f64);
    }
    for i in 50_000..100_000 {
        b.add(i as f64);
        combined.add(i as f64);
    }
    let m = a.merge(&b);
    assert_eq!(m.count(), 100_000.0);
    // merged + directly-combined should agree closely on quantiles
    for &q in &[0.1, 0.5, 0.9, 0.99] {
        let diff = (m.quantile(q) - combined.quantile(q)).abs();
        assert!(
            diff < 500.0,
            "q={q}: merge={} combined={} diff={diff}",
            m.quantile(q),
            combined.quantile(q)
        );
    }
}

#[test]
fn centroid_count_bounded() {
    // compression bounds the centroid count regardless of input size.
    let mut d = TsTDigest::new(100.0);
    for i in 0..1_000_000 {
        d.add((i % 1000) as f64);
    }
    let bytes = d.serialize();
    // header 29 bytes + 16/centroid; centroid count stays ~O(compression)
    let centroids = (bytes.len() - 29) / 16;
    assert!(centroids < 1_000, "centroids={centroids} should be bounded");
}

#[test]
fn serialize_roundtrip() {
    let mut d = TsTDigest::new(150.0);
    for i in 0..20_000 {
        d.add((i as f64 * 0.001).sin() * 100.0);
    }
    let bytes = d.serialize();
    let back = TsTDigest::deserialize(&bytes).unwrap();
    assert_eq!(back.count(), d.count());
    for &q in &[0.01, 0.25, 0.5, 0.75, 0.99] {
        assert!((back.quantile(q) - d.quantile(q)).abs() < 1e-9);
    }
    // re-serialize is byte-identical
    assert_eq!(back.serialize(), bytes);
}

#[test]
fn deserialize_bad_version() {
    let mut d = TsTDigest::new(100.0);
    d.add(1.0);
    let mut bytes = d.serialize();
    bytes[0] = 9;
    assert!(matches!(
        TsTDigest::deserialize(&bytes),
        Err(TsTDigestError::BadVersion(9))
    ));
}
