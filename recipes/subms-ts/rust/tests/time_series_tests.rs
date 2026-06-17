use subms_ts::{TsError, TsPoint, TsSeries};

fn seeded(points: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(ts, v) in points {
        s.push(ts, v).unwrap();
    }
    s
}

#[test]
fn push_and_len() {
    let s = seeded(&[(1, 1.0), (2, 2.0), (3, 3.0)]);
    assert_eq!(s.len(), 3);
    assert!(!s.is_empty());
    assert_eq!(s.first().unwrap(), TsPoint::new(1, 1.0));
    assert_eq!(s.last().unwrap(), TsPoint::new(3, 3.0));
}

#[test]
fn empty_series_queries_are_none() {
    let s = TsSeries::<f64>::new();
    assert!(s.is_empty());
    assert_eq!(s.first(), None);
    assert_eq!(s.last(), None);
    assert_eq!(s.nearest(0), None);
    assert_eq!(s.min(), None);
    assert_eq!(s.mean(), None);
}

#[test]
fn push_rejects_out_of_order() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 1.0).unwrap();
    assert_eq!(
        s.push(5, 2.0),
        Err(TsError::NotMonotonic { last: 10, got: 5 })
    );
    // equal ts is allowed (non-decreasing)
    assert!(s.push(10, 3.0).is_ok());
}

#[test]
fn push_rejects_nan_and_inf() {
    let mut s = TsSeries::<f64>::new();
    assert!(matches!(
        s.push(1, f64::NAN),
        Err(TsError::NullValue { .. })
    ));
    assert!(matches!(
        s.push(1, f64::INFINITY),
        Err(TsError::NullValue { .. })
    ));
    assert_eq!(s.len(), 0);
}

#[test]
fn get_at_exact_and_miss() {
    let s = seeded(&[(1, 1.0), (3, 3.0), (5, 5.0)]);
    assert_eq!(s.get_at(3).unwrap().value, 3.0);
    assert_eq!(s.get_at(4), None);
    assert_eq!(s.get_at(0), None);
    assert_eq!(s.get_at(6), None);
}

#[test]
fn nearest_before_after_and_nearest() {
    let s = seeded(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(s.nearest_before(25).unwrap().ts, 20);
    assert_eq!(s.nearest_before(10).unwrap().ts, 10);
    assert_eq!(s.nearest_before(5), None);
    assert_eq!(s.nearest_after(25).unwrap().ts, 30);
    assert_eq!(s.nearest_after(30).unwrap().ts, 30);
    assert_eq!(s.nearest_after(31), None);
    assert_eq!(s.nearest(24).unwrap().ts, 20);
    assert_eq!(s.nearest(26).unwrap().ts, 30);
    // tie resolves to the earlier
    assert_eq!(s.nearest(25).unwrap().ts, 20);
}

#[test]
fn range_inclusive_bounds_and_empty() {
    let s = seeded(&[(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0)]);
    let got: Vec<i64> = s.range(2, 3).map(|p| p.ts).collect();
    assert_eq!(got, vec![2, 3]);
    assert_eq!(s.range(5, 9).count(), 0);
    assert_eq!(s.range(3, 1).count(), 0); // lo > hi
    assert_eq!(s.range(0, 100).count(), 4);
}

#[test]
fn aggregates_full_and_ranged() {
    let s = seeded(&[(1, 5.0), (2, 1.0), (3, 9.0), (4, 3.0)]);
    assert_eq!(s.min(), Some(1.0));
    assert_eq!(s.max(), Some(9.0));
    assert_eq!(s.sum(), 18.0);
    assert_eq!(s.mean(), Some(4.5));
    assert_eq!(s.min_point().unwrap().ts, 2);
    assert_eq!(s.max_point().unwrap().ts, 3);
    assert_eq!(s.range_min(2, 3), Some(1.0));
    assert_eq!(s.range_max(2, 3), Some(9.0));
    assert_eq!(s.range_sum(2, 3), 10.0);
    assert_eq!(s.range_mean(2, 3), Some(5.0));
}

#[test]
fn delete_at_and_range() {
    let mut s = seeded(&[(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0)]);
    assert_eq!(s.delete_at(2).unwrap().value, 2.0);
    assert_eq!(s.len(), 3);
    assert_eq!(s.get_at(2), None);
    assert_eq!(s.delete_range(3, 4), 2);
    assert_eq!(s.len(), 1);
    assert_eq!(s.first().unwrap().ts, 1);
    assert_eq!(s.last().unwrap().ts, 1);
}

#[test]
fn delete_by_value_and_value_range() {
    let mut s = seeded(&[(1, 5.0), (2, 1.0), (3, 5.0), (4, 9.0)]);
    assert_eq!(s.delete_by_value(&5.0), 2);
    assert_eq!(s.len(), 2);
    let mut s2 = seeded(&[(1, 1.0), (2, 4.0), (3, 7.0), (4, 10.0)]);
    assert_eq!(s2.delete_value_range(&4.0, &7.0), 2);
    assert_eq!(
        s2.iter().map(|p| p.value).collect::<Vec<_>>(),
        vec![1.0, 10.0]
    );
}

#[test]
fn truncate_retain_pop_clear() {
    let mut s = seeded(&[(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)]);
    assert_eq!(s.truncate_before(3), 2);
    assert_eq!(s.first().unwrap().ts, 3);
    assert_eq!(s.truncate_after(4), 1);
    assert_eq!(s.last().unwrap().ts, 4);
    assert_eq!(s.retain(|p| p.value != 3.0), 1);
    assert_eq!(s.len(), 1);
    assert_eq!(s.pop_first().unwrap().ts, 4);
    assert!(s.is_empty());
    let mut s2 = seeded(&[(1, 1.0), (2, 2.0)]);
    assert_eq!(s2.pop_last().unwrap().ts, 2);
    s2.clear();
    assert!(s2.is_empty());
}

#[test]
fn from_points_validates() {
    let ok = TsSeries::from_points(vec![TsPoint::new(1, 1.0), TsPoint::new(2, 2.0)]);
    assert!(ok.is_ok());
    let bad = TsSeries::from_points(vec![TsPoint::new(2, 1.0), TsPoint::new(1, 2.0)]);
    assert!(matches!(bad, Err(TsError::NotMonotonic { .. })));
}

#[test]
fn seal_boundary_is_transparent() {
    // Cross the 64Ki seal threshold: queries + aggregates must stay correct
    // across the warm/head chunk boundary.
    let n = 70_000i64;
    let mut s = TsSeries::<i64>::new();
    for i in 0..n {
        s.push(i, i).unwrap();
    }
    assert_eq!(s.len() as i64, n);
    assert_eq!(s.first().unwrap().ts, 0);
    assert_eq!(s.last().unwrap().ts, n - 1);
    // a point inside the sealed warm tier
    assert_eq!(s.get_at(100).unwrap().value, 100);
    // a point in the live head tier
    assert_eq!(s.get_at(69_000).unwrap().value, 69_000);
    // a range spanning the boundary (65_536)
    let span: Vec<i64> = s.range(65_530, 65_540).map(|p| p.ts).collect();
    assert_eq!(span, (65_530..=65_540).collect::<Vec<_>>());
    assert_eq!(s.nearest_before(65_536).unwrap().ts, 65_536);
    assert_eq!(s.max(), Some(n - 1));
    assert_eq!(s.range_sum(0, 9), 45);
}

#[test]
fn delete_across_seal_boundary_rechunks() {
    let mut s = TsSeries::<i64>::new();
    for i in 0..70_000i64 {
        s.push(i, i).unwrap();
    }
    let removed = s.delete_range(0, 50_000);
    assert_eq!(removed, 50_001);
    assert_eq!(s.len(), 70_000 - 50_001);
    assert_eq!(s.first().unwrap().ts, 50_001);
    assert_eq!(s.last().unwrap().ts, 69_999);
    // surviving points still queryable + ordered
    assert_eq!(s.get_at(60_000).unwrap().value, 60_000);
}

// Aggregates must agree with an independent scalar reference whether or not
// the `simd` feature is on. Spans multiple chunks (> SEAL_CAP) so the
// per-slice kernels run across the warm + head boundary, and includes the
// sub-LANES remainder path.
#[test]
fn aggregates_match_reference_across_chunks() {
    const N: i64 = 150_003; // > 2 * SEAL_CAP, not a multiple of 8
    let mut s = TsSeries::<f64>::with_capacity(N as usize);
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut vals = Vec::with_capacity(N as usize);
    for i in 0..N {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let v = ((state >> 11) as f64 / (1u64 << 53) as f64) * 200.0 - 100.0;
        vals.push(v);
        s.push(i, v).unwrap();
    }

    let ref_sum: f64 = vals.iter().copied().sum();
    let ref_min = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let ref_max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    // min/max are exact regardless of reduction order.
    assert_eq!(s.min().unwrap(), ref_min);
    assert_eq!(s.max().unwrap(), ref_max);
    // sum may reorder by lane under `simd`; allow a relative ULP slack.
    let tol = ref_sum.abs() * 1e-12 + 1e-9;
    assert!(
        (s.sum() - ref_sum).abs() <= tol,
        "sum {} vs {}",
        s.sum(),
        ref_sum
    );
    assert!((s.mean().unwrap() - ref_sum / N as f64).abs() <= tol);

    // ranged variants over a window that crosses a chunk seam.
    let (lo, hi) = (60_000i64, 130_000i64);
    let win: Vec<f64> = (lo..=hi).map(|i| vals[i as usize]).collect();
    let rsum: f64 = win.iter().copied().sum();
    assert_eq!(
        s.range_min(lo, hi).unwrap(),
        win.iter().copied().fold(f64::INFINITY, f64::min)
    );
    assert_eq!(
        s.range_max(lo, hi).unwrap(),
        win.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    );
    let rtol = rsum.abs() * 1e-12 + 1e-9;
    assert!((s.range_sum(lo, hi) - rsum).abs() <= rtol);
}

// The kernels handle slices shorter than one lane group (the scalar tail).
#[test]
fn aggregates_short_series() {
    let s = seeded(&[(1, 5.0), (2, -3.0), (3, 7.5)]);
    assert_eq!(s.min().unwrap(), -3.0);
    assert_eq!(s.max().unwrap(), 7.5);
    assert!((s.sum() - 9.5).abs() < 1e-12);
}
