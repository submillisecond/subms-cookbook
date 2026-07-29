use super::*;

#[test]
fn percentile_empty_is_zero() {
    assert_eq!(percentile(&[], 0.5), 0);
}

#[test]
fn percentile_known_distribution() {
    let mut v: Vec<u64> = (0..100).collect();
    v.sort_unstable();
    assert_eq!(percentile(&v, 0.50), 50);
    assert_eq!(percentile(&v, 0.99), 99);
    assert_eq!(percentile(&v, 1.0), 99);
}

#[test]
fn percentile_sweep_endpoints_included() {
    let v: Vec<u64> = (0..100).collect();
    let sweep = percentile_sweep(&v, 0.0, 1.0, 0.5);
    assert_eq!(sweep.len(), 3);
    assert_eq!(sweep[0].0, 0.0);
    assert_eq!(sweep[2].0, 1.0);
}

#[test]
fn percentile_sweep_rejects_zero_step() {
    let v: Vec<u64> = (0..100).collect();
    assert!(percentile_sweep(&v, 0.0, 1.0, 0.0).is_empty());
}

#[test]
fn mean_stddev_basic() {
    let samples = vec![100u64, 200, 300, 400];
    assert_eq!(mean(&samples), 250);
    let sd = stddev(&samples);
    assert!((125..=135).contains(&sd), "stddev around 129: {}", sd);
}

#[test]
fn mean_empty_zero() {
    assert_eq!(mean(&[]), 0);
}

#[test]
fn stddev_single_sample_zero() {
    assert_eq!(stddev(&[42]), 0);
}
