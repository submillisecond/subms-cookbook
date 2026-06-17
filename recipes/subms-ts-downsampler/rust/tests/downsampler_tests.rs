use subms_ts_downsampler::TsDownsampler;

#[test]
fn single_tier_buckets() {
    let mut d = TsDownsampler::new(&[10]);
    for ts in 0..25 {
        d.push(ts, ts as f64);
    }
    d.flush();
    // buckets [0,10), [10,20), [20,30) -> 3 closed buckets
    assert_eq!(d.tier(0).len(), 3);
    let pts: Vec<(i64, f64)> = d.tier(0).iter().map(|p| (p.ts, p.value)).collect();
    assert_eq!(pts[0], (0, 4.5)); // mean of 0..9
    assert_eq!(pts[1], (10, 14.5));
    assert_eq!(pts[2], (20, 22.0)); // mean of 20..24
}

#[test]
fn multi_tier_independent_buckets() {
    let mut d = TsDownsampler::new(&[10, 100]);
    for ts in 0..250 {
        d.push(ts, 1.0);
    }
    d.flush();
    assert_eq!(d.tier(0).len(), 25); // 10-ns buckets
    assert_eq!(d.tier(1).len(), 3); // 100-ns buckets: [0,100),[100,200),[200,300)
    assert_eq!(d.tier_count(), 2);
}

#[test]
fn bucket_stats_full() {
    let mut d = TsDownsampler::new(&[100]);
    for &(ts, v) in &[(0i64, 5.0), (10, 1.0), (20, 9.0), (30, 3.0)] {
        d.push(ts, v);
    }
    // bucket [0,100) is still open
    let s = d.bucket_stats(0, 50).unwrap();
    assert_eq!(s.count, 4);
    assert_eq!(s.sum, 18.0);
    assert_eq!(s.min, 1.0);
    assert_eq!(s.max, 9.0);
    assert_eq!(s.last, 3.0);
    assert_eq!(s.mean(), 4.5);
}

#[test]
fn bucket_stats_closed_lookup() {
    let mut d = TsDownsampler::new(&[10]);
    for ts in 0..25 {
        d.push(ts, ts as f64);
    }
    // bucket [10,20) is closed once ts 20 arrives
    let s = d.bucket_stats(0, 15).unwrap();
    assert_eq!(s.count, 10);
    assert_eq!(s.min, 10.0);
    assert_eq!(s.max, 19.0);
    assert_eq!(s.last, 19.0);
}

#[test]
fn empty_bucket_is_none() {
    let mut d = TsDownsampler::new(&[10]);
    d.push(0, 1.0);
    assert!(d.bucket_stats(0, 1_000).is_none());
}

#[test]
fn sparse_points_skip_empty_buckets() {
    // points at 0 and 500 with 100-ns buckets: only buckets [0,100) and
    // [500,600) exist; the gap buckets are never created.
    let mut d = TsDownsampler::new(&[100]);
    d.push(0, 1.0);
    d.push(500, 2.0);
    d.flush();
    assert_eq!(d.tier(0).len(), 2);
    let pts: Vec<i64> = d.tier(0).iter().map(|p| p.ts).collect();
    assert_eq!(pts, vec![0, 500]);
}

#[test]
fn flush_emits_open_bucket() {
    let mut d = TsDownsampler::new(&[100]);
    d.push(0, 1.0);
    d.push(50, 3.0);
    assert_eq!(d.tier(0).len(), 0); // open, not yet emitted
    d.flush();
    assert_eq!(d.tier(0).len(), 1);
    assert_eq!(d.tier(0).first().unwrap().value, 2.0); // mean of 1,3
}

#[test]
fn tier_durations_reported() {
    let d = TsDownsampler::new(&[1_000_000_000, 60_000_000_000]);
    assert_eq!(d.tier_duration(0), 1_000_000_000);
    assert_eq!(d.tier_duration(1), 60_000_000_000);
}

#[test]
fn negative_timestamps_bucket_correctly() {
    // div_euclid keeps buckets aligned for negative ts
    let mut d = TsDownsampler::new(&[10]);
    d.push(-15, 1.0); // bucket [-20,-10)
    d.push(-12, 2.0);
    d.push(-5, 3.0); // bucket [-10,0)
    d.flush();
    assert_eq!(d.tier(0).len(), 2);
    let pts: Vec<i64> = d.tier(0).iter().map(|p| p.ts).collect();
    assert_eq!(pts, vec![-20, -10]);
}

#[test]
fn realistic_tiers_1s_1m() {
    const S: i64 = 1_000_000_000;
    const M: i64 = 60 * S;
    let mut d = TsDownsampler::new(&[S, M]);
    // 1 point/100ms for 3 minutes
    let mut ts = 0i64;
    while ts < 3 * M {
        d.push(ts, (ts / S) as f64);
        ts += 100_000_000;
    }
    d.flush();
    assert_eq!(d.tier(0).len(), 180); // 180 one-second buckets
    assert_eq!(d.tier(1).len(), 3); // 3 one-minute buckets
}
