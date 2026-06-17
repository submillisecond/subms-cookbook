use subms_ts_anomaly::TsAnomalyDetector;

#[test]
fn warmup_returns_none() {
    let mut d = TsAnomalyDetector::new(1_000, 3.0);
    assert!(d.push(0, 5.0).is_none()); // n=0 prior
    assert!(d.push(1, 5.0).is_none()); // n=1 prior
    // third push has 2 prior points -> can score
    assert!(d.push(2, 5.0).is_none()); // still stable
}

#[test]
fn stable_series_no_anomalies() {
    let mut d = TsAnomalyDetector::new(10_000, 3.0);
    let mut flags = 0;
    for i in 0..200 {
        // alternating 9 / 11: mean 10, std ~ 1, every value ~1 sigma out -
        // a real band with healthy variance, never 3 sigma.
        let v = 10.0 + if i % 2 == 0 { -1.0 } else { 1.0 };
        if d.push(i, v).is_some() {
            flags += 1;
        }
    }
    assert_eq!(flags, 0);
}

#[test]
fn spike_flagged() {
    let mut d = TsAnomalyDetector::new(10_000, 3.0);
    for i in 0..50 {
        d.push(i, 10.0 + (i % 2) as f64 * 0.1);
    }
    let hit = d.push(50, 100.0);
    assert!(hit.is_some());
    let a = hit.unwrap();
    assert_eq!(a.ts, 50);
    assert_eq!(a.value, 100.0);
    assert!(a.zscore > 3.0, "z={}", a.zscore);
}

#[test]
fn jump_off_flat_baseline_flags() {
    // constant baseline (std ~ 0): a jump must still flag, not divide by zero
    let mut d = TsAnomalyDetector::new(10_000, 3.0);
    for i in 0..30 {
        d.push(i, 7.0);
    }
    let hit = d.push(30, 8.0);
    assert!(hit.is_some());
    assert!(hit.unwrap().zscore.is_finite());
}

#[test]
fn negative_spike_has_negative_z() {
    let mut d = TsAnomalyDetector::new(10_000, 3.0);
    for i in 0..50 {
        d.push(i, 100.0 + (i % 2) as f64 * 0.1);
    }
    let a = d.push(50, 1.0).expect("downward spike should flag");
    assert!(a.zscore < -3.0, "z={}", a.zscore);
}

#[test]
fn sigma_threshold_respected() {
    // build a baseline with known mean/std, then probe just under/over sigma
    let mut d = TsAnomalyDetector::new(1_000_000, 2.0);
    // values 0..100 -> mean ~ 49.5, std ~ 28.9 over a wide window
    for i in 0..100 {
        d.push(i, i as f64);
    }
    // a value ~1 std above mean should NOT flag at 2 sigma
    assert!(d.push(100, 78.0).is_none());
    let mut d2 = d.clone();
    // a value ~3 std above should flag
    assert!(d2.push(101, 140.0).is_some());
}

#[test]
fn window_expiry_shifts_baseline() {
    let mut d = TsAnomalyDetector::new(100, 3.0);
    // old regime around 0
    for i in 0..50 {
        d.push(i, 0.0 + (i % 2) as f64 * 0.01);
    }
    // jump to a new regime; first few flag, then the window fills with the
    // new level and stops flagging once the old points expire
    let mut later_flags = 0;
    for i in 200..300 {
        if d.push(i, 50.0 + (i % 2) as f64 * 0.01).is_some() {
            later_flags += 1;
        }
    }
    // by the end the baseline has fully shifted to ~50 -> no flags
    assert_eq!(
        later_flags, 0,
        "baseline should have adapted to the new regime"
    );
}

#[test]
fn window_count_tracks() {
    let mut d = TsAnomalyDetector::new(100, 3.0);
    d.push(0, 1.0);
    d.push(50, 2.0);
    d.push(90, 3.0);
    assert_eq!(d.window_count(), 3);
    d.push(201, 4.0); // cutoff 101 -> 0,50,90 all expire
    assert_eq!(d.window_count(), 1);
}

#[test]
fn flat_baseline_same_value_no_flag() {
    let mut d = TsAnomalyDetector::new(10_000, 3.0);
    for i in 0..30 {
        d.push(i, 7.0);
    }
    // same value as the flat baseline -> z = 0, no flag
    assert!(d.push(30, 7.0).is_none());
}

#[test]
fn cross_check_zscore_against_manual() {
    let mut d = TsAnomalyDetector::new(1_000_000, 0.0); // sigma 0 -> always "flag", read z
    d.push(0, 2.0);
    d.push(1, 4.0);
    // prior window {2,4}: mean 3, var = (4+16)/2 - 9 = 1, std 1
    let a = d.push(2, 6.0).unwrap();
    assert!((a.zscore - 3.0).abs() < 1e-9, "z={}", a.zscore); // (6-3)/1
}
