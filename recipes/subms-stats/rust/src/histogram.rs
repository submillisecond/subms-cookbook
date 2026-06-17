//! Log2-spaced CDF buckets. Behind the `histogram` Cargo feature
//! (on by default).
//!
//! Use to export a full empirical distribution alongside the headline
//! p50/p99 percentiles - downstream tooling can reconstruct any
//! quantile from the bucket cumulative sums without needing the raw
//! sample stream.

/// Log2-spaced histogram of latencies. 64 buckets covering
/// `[2^i, 2^(i+1))` nanoseconds for `i in 0..64`. A full empirical
/// CDF can be reconstructed from the cumulative sum of this array.
/// Empty input -> all-zero buckets.
///
/// Bucket bounds:
///
/// | Bucket | Range | Range (human) |
/// |---|---|---|
/// | 0 | [0, 2) ns | sub-ns |
/// | 6 | [64, 128) ns | tens of ns |
/// | 10 | [1024, 2048) ns | ~1 us |
/// | 20 | [1048576, 2097152) ns | ~1 ms |
/// | 30 | [~1.07e9, ~2.15e9) ns | ~1 s |
/// | 63 | [2^63, 2^64) ns | ~292 years |
pub fn cdf_buckets(samples: &[u64]) -> Vec<u64> {
    let mut buckets = vec![0u64; 64];
    for &v in samples {
        let idx = if v == 0 {
            0
        } else {
            // Highest set bit of v: log2 floor.
            (63 - v.leading_zeros()) as usize
        };
        let idx = idx.min(63);
        buckets[idx] = buckets[idx].saturating_add(1);
    }
    buckets
}

#[cfg(test)]
mod tests {
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
}
