use super::*;

#[test]
fn cte_exceeds_quantile() {
    let v: Vec<u64> = (0..100).collect();
    let cte99 = conditional_tail_expectation(&v, 0.95);
    assert!(cte99 >= 95);
}

#[test]
fn cte_empty_is_zero() {
    assert_eq!(conditional_tail_expectation(&[], 0.99), 0);
}

#[test]
fn hill_returns_none_for_tiny_input() {
    assert!(hill_tail_index(&[1, 2, 3], 5).is_none());
}

#[test]
fn hill_powerlike_tail_returns_positive() {
    let v: Vec<u64> = (1..1000).map(|i| (i as u64).pow(2)).collect();
    let idx = hill_tail_index(&v, 50).unwrap();
    assert!(
        idx > 0.0,
        "Hill estimator on power-law tail should be positive: {}",
        idx
    );
}

#[test]
fn fatness_ratio_uniform_close_to_one() {
    let v = vec![100u64; 1000];
    let r = tail_fatness_ratio(&v);
    assert!((r - 1.0).abs() < 0.01);
}

#[test]
fn fatness_ratio_heavy_tail_exceeds_one() {
    let mut v: Vec<u64> = vec![100; 990];
    v.resize(v.len() + 10, 10_000);
    let r = tail_fatness_ratio(&v);
    assert!(r > 1.0);
}
