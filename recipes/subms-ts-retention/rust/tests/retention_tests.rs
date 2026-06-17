use subms_ts::TsSeries;
use subms_ts_retention::{BYTES_PER_POINT, TsRetentionPolicy};

fn series(n: i64) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for i in 0..n {
        s.push(i, i as f64).unwrap();
    }
    s
}

#[test]
fn empty_policy_is_noop() {
    let mut s = series(100);
    let removed = TsRetentionPolicy::new().apply(&mut s);
    assert_eq!(removed, 0);
    assert_eq!(s.len(), 100);
}

#[test]
fn count_keeps_newest() {
    let mut s = series(1_000);
    let removed = TsRetentionPolicy::new().max_points(100).apply(&mut s);
    assert_eq!(removed, 900);
    assert_eq!(s.len(), 100);
    assert_eq!(s.first().unwrap().ts, 900);
    assert_eq!(s.last().unwrap().ts, 999);
}

#[test]
fn count_under_limit_noop() {
    let mut s = series(50);
    let removed = TsRetentionPolicy::new().max_points(100).apply(&mut s);
    assert_eq!(removed, 0);
    assert_eq!(s.len(), 50);
}

#[test]
fn count_zero_clears() {
    let mut s = series(10);
    let removed = TsRetentionPolicy::new().max_points(0).apply(&mut s);
    assert_eq!(removed, 10);
    assert!(s.is_empty());
}

#[test]
fn age_keeps_within_window() {
    // ts 0..1000, latest 999, age 100 -> keep ts >= 899.
    let mut s = series(1_000);
    let removed = TsRetentionPolicy::new().max_age_ns(100).apply(&mut s);
    assert_eq!(s.first().unwrap().ts, 899);
    assert_eq!(s.last().unwrap().ts, 999);
    assert_eq!(removed, 899);
    assert_eq!(s.len(), 101); // inclusive boundary
}

#[test]
fn age_boundary_inclusive() {
    let mut s = series(10); // 0..9, latest 9
    // age 5 -> cutoff 4, keep ts >= 4 -> {4,5,6,7,8,9} = 6
    let removed = TsRetentionPolicy::new().max_age_ns(5).apply(&mut s);
    assert_eq!(removed, 4);
    assert_eq!(s.len(), 6);
    assert_eq!(s.first().unwrap().ts, 4);
}

#[test]
fn age_larger_than_span_noop() {
    let mut s = series(100);
    let removed = TsRetentionPolicy::new().max_age_ns(1_000_000).apply(&mut s);
    assert_eq!(removed, 0);
    assert_eq!(s.len(), 100);
}

#[test]
fn bytes_caps_point_count() {
    let mut s = series(1_000);
    // budget for 50 points
    let removed = TsRetentionPolicy::new()
        .max_bytes(50 * BYTES_PER_POINT)
        .apply(&mut s);
    assert_eq!(s.len(), 50);
    assert_eq!(removed, 950);
    assert_eq!(s.last().unwrap().ts, 999);
}

#[test]
fn point_cap_is_tighter_of_count_and_bytes() {
    let p = TsRetentionPolicy::new()
        .max_points(200)
        .max_bytes(50 * BYTES_PER_POINT);
    assert_eq!(p.point_cap(), Some(50));
    let p2 = TsRetentionPolicy::new()
        .max_points(30)
        .max_bytes(50 * BYTES_PER_POINT);
    assert_eq!(p2.point_cap(), Some(30));
}

#[test]
fn age_then_count_most_restrictive() {
    let mut s = series(1_000);
    // age keeps ts>=900 (100 points), count then keeps newest 20.
    let removed = TsRetentionPolicy::new()
        .max_age_ns(100)
        .max_points(20)
        .apply(&mut s);
    assert_eq!(s.len(), 20);
    assert_eq!(s.first().unwrap().ts, 980);
    assert_eq!(s.last().unwrap().ts, 999);
    assert_eq!(removed, 980);
}

#[test]
fn empty_series_noop() {
    let mut s = TsSeries::<f64>::new();
    let removed = TsRetentionPolicy::new().max_points(10).max_age_ns(5).apply(&mut s);
    assert_eq!(removed, 0);
    assert!(s.is_empty());
}

#[test]
fn apply_all_folds_over_series() {
    let mut a = series(500);
    let mut b = series(300);
    let policy = TsRetentionPolicy::new().max_points(100);
    let removed = policy.apply_all([&mut a, &mut b]);
    assert_eq!(removed, 400 + 200);
    assert_eq!(a.len(), 100);
    assert_eq!(b.len(), 100);
}

#[test]
fn works_on_i64_series() {
    let mut s = TsSeries::<i64>::new();
    for i in 0..200 {
        s.push(i, i * 2).unwrap();
    }
    let removed = TsRetentionPolicy::new().max_points(64).apply(&mut s);
    assert_eq!(removed, 136);
    assert_eq!(s.len(), 64);
    assert_eq!(s.last().unwrap().value, 199 * 2);
}

#[test]
fn crosses_chunk_boundary() {
    // > SEAL_CAP so the prune spans warm + head chunks.
    let mut s = series(150_000);
    let removed = TsRetentionPolicy::new().max_points(1_000).apply(&mut s);
    assert_eq!(removed, 149_000);
    assert_eq!(s.len(), 1_000);
    assert_eq!(s.first().unwrap().ts, 149_000);
}
