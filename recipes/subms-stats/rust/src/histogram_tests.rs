use super::*;

#[test]
fn log2_placement() {
    let buckets = cdf_buckets(&[1, 2, 3, 4, 8, 100, 1_000_000]);
    assert_eq!(buckets[0], 1);
    assert_eq!(buckets[1], 2);
    assert_eq!(buckets[2], 1);
    assert_eq!(buckets[3], 1);
    assert_eq!(buckets[6], 1);
    assert_eq!(buckets[19], 1);
}

#[test]
fn empty_input_all_zero() {
    let buckets = cdf_buckets(&[]);
    assert_eq!(buckets.len(), 64);
    assert!(buckets.iter().all(|&c| c == 0));
}

#[test]
fn cdf_buckets_count_matches_total() {
    let raw: Vec<u64> = (1..=1000).collect();
    let buckets = cdf_buckets(&raw);
    let total: u64 = buckets.iter().sum();
    assert_eq!(total as usize, raw.len());
}

#[test]
fn zero_value_lands_in_bucket_zero() {
    let buckets = cdf_buckets(&[0, 0, 0]);
    assert_eq!(buckets[0], 3);
}
