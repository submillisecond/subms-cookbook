use super::*;

#[test]
fn p99_ci_brackets_point_estimate() {
    let v: Vec<u64> = (0..1000).collect();
    let (lo, hi) = bootstrap_percentile_ci(&v, 0.99, 200, 0.95, 42);
    let point = {
        let mut s = v.clone();
        s.sort_unstable();
        percentile(&s, 0.99)
    };
    assert!(lo <= point && point <= hi);
}

#[test]
fn empty_returns_zero_pair() {
    assert_eq!(bootstrap_percentile_ci(&[], 0.99, 100, 0.95, 0), (0, 0));
}

#[test]
fn deterministic_under_same_seed() {
    let v: Vec<u64> = (0..200).collect();
    let a = bootstrap_percentile_ci(&v, 0.99, 100, 0.95, 7);
    let b = bootstrap_percentile_ci(&v, 0.99, 100, 0.95, 7);
    assert_eq!(a, b);
}

#[test]
fn zero_iters_returns_zero_pair() {
    let v: Vec<u64> = (0..100).collect();
    assert_eq!(bootstrap_percentile_ci(&v, 0.99, 0, 0.95, 0), (0, 0));
}
