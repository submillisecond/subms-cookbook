use super::*;

#[test]
fn iqr_known_distribution() {
    let v: Vec<u64> = (0..100).collect();
    assert_eq!(iqr(&v), 50);
}

#[test]
fn mad_basic() {
    let v: Vec<u64> = (0..100).collect();
    let mad = median_absolute_deviation(&v);
    assert!((20..=30).contains(&mad), "MAD around 25: {}", mad);
}

#[test]
fn cov_constant_signal_is_zero() {
    let v = vec![100u64; 100];
    assert!(coefficient_of_variation(&v) < 0.001);
}

#[test]
fn skewness_right_tail_positive() {
    let mut v: Vec<u64> = vec![100; 990];
    v.resize(v.len() + 10, 10_000);
    assert!(skewness(&v) > 0.0);
}

#[test]
fn kurtosis_heavy_tail_positive() {
    let mut v: Vec<u64> = vec![100; 990];
    v.resize(v.len() + 10, 10_000);
    assert!(kurtosis(&v) > 0.0);
}

#[test]
fn iqr_empty_zero() {
    assert_eq!(iqr(&[]), 0);
}
