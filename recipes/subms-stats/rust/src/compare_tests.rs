use super::*;

#[test]
fn ks_same_distribution_near_zero() {
    let a: Vec<u64> = (0..1000).collect();
    let b: Vec<u64> = (0..1000).collect();
    assert!(ks_statistic(&a, &b).unwrap() < 0.01);
}

#[test]
fn ks_shifted_distribution_large() {
    let a: Vec<u64> = (0..1000).collect();
    let b: Vec<u64> = (500..1500).collect();
    assert!(ks_statistic(&a, &b).unwrap() > 0.4);
}

#[test]
fn ks_empty_returns_none() {
    assert!(ks_statistic(&[], &[1, 2]).is_none());
    assert!(ks_statistic(&[1, 2], &[]).is_none());
}

#[test]
fn cohens_d_zero_for_identical() {
    let a: Vec<u64> = (0..100).collect();
    let b: Vec<u64> = (0..100).collect();
    assert!(cohens_d(&a, &b).unwrap().abs() < 0.01);
}

#[test]
fn cohens_d_positive_when_candidate_slower() {
    let a: Vec<u64> = (100..200).collect();
    let b: Vec<u64> = (200..300).collect();
    assert!(cohens_d(&a, &b).unwrap() > 0.5);
}
