use subms_ts::TsSeries;
use subms_ts_resample::{TsResampleMode, resample_to_grid};

fn series(pts: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(t, v) in pts {
        s.push(t, v).unwrap();
    }
    s
}

fn grid(s: &TsSeries<f64>, period: i64, mode: TsResampleMode) -> Vec<(i64, f64)> {
    resample_to_grid(s, period, mode)
        .iter()
        .map(|p| (p.ts, p.value))
        .collect()
}

#[test]
fn mean_buckets() {
    let s = series(&[(0, 1.0), (3, 3.0), (11, 5.0), (19, 7.0)]);
    assert_eq!(
        grid(&s, 10, TsResampleMode::Mean),
        vec![(0, 2.0), (10, 6.0)]
    );
}

#[test]
fn last_and_first() {
    let s = series(&[(0, 1.0), (3, 3.0), (9, 9.0), (11, 5.0)]);
    assert_eq!(
        grid(&s, 10, TsResampleMode::Last),
        vec![(0, 9.0), (10, 5.0)]
    );
    assert_eq!(
        grid(&s, 10, TsResampleMode::First),
        vec![(0, 1.0), (10, 5.0)]
    );
}

#[test]
fn sum_and_count() {
    let s = series(&[(0, 1.0), (3, 2.0), (5, 3.0), (11, 4.0)]);
    assert_eq!(grid(&s, 10, TsResampleMode::Sum), vec![(0, 6.0), (10, 4.0)]);
    assert_eq!(
        grid(&s, 10, TsResampleMode::Count),
        vec![(0, 3.0), (10, 1.0)]
    );
}

#[test]
fn min_and_max() {
    let s = series(&[(0, 5.0), (3, 1.0), (9, 9.0), (11, 7.0)]);
    assert_eq!(grid(&s, 10, TsResampleMode::Min), vec![(0, 1.0), (10, 7.0)]);
    assert_eq!(grid(&s, 10, TsResampleMode::Max), vec![(0, 9.0), (10, 7.0)]);
}

#[test]
fn empty_series() {
    let s = TsSeries::<f64>::new();
    assert!(resample_to_grid(&s, 10, TsResampleMode::Mean).is_empty());
}

#[test]
fn single_point() {
    let s = series(&[(7, 42.0)]);
    assert_eq!(grid(&s, 10, TsResampleMode::Mean), vec![(0, 42.0)]);
}

#[test]
fn period_zero_empty() {
    let s = series(&[(0, 1.0), (5, 2.0)]);
    assert!(resample_to_grid(&s, 0, TsResampleMode::Mean).is_empty());
}

#[test]
fn sparse_buckets_no_empties() {
    // points only in buckets [0,10) and [50,60): no empty buckets emitted
    let s = series(&[(0, 1.0), (5, 2.0), (55, 9.0)]);
    let g = grid(&s, 10, TsResampleMode::Mean);
    assert_eq!(g, vec![(0, 1.5), (50, 9.0)]);
}

#[test]
fn bucket_alignment_absolute() {
    // buckets anchored to absolute time, not the first point
    let s = series(&[(7, 1.0), (13, 2.0)]);
    // 7 -> bucket [0,10), 13 -> bucket [10,20)
    assert_eq!(
        grid(&s, 10, TsResampleMode::First),
        vec![(0, 1.0), (10, 2.0)]
    );
}

#[test]
fn negative_ts_buckets() {
    let s = series(&[(-15, 1.0), (-12, 2.0), (-5, 3.0)]);
    // -15,-12 -> [-20,-10); -5 -> [-10,0)
    assert_eq!(
        grid(&s, 10, TsResampleMode::Mean),
        vec![(-20, 1.5), (-10, 3.0)]
    );
}

#[test]
fn output_strictly_increasing() {
    let s = series(&(0..500).map(|i| (i * 7, i as f64)).collect::<Vec<_>>());
    let g = grid(&s, 100, TsResampleMode::Mean);
    for w in g.windows(2) {
        assert!(w[1].0 > w[0].0);
    }
}
