//! Histogram with per-recording 1-byte tags.
//!
//! Useful when you want to slice latencies by category at query
//! time (request method, endpoint, tenant tier) without paying for
//! N separate histograms.
//!
//! Storage: each base bucket carries a small per-tag map. The total
//! count is the sum across all tags. Per-tag percentiles iterate
//! over buckets and accumulate only the matching tag's count.
//!
//! Trade-off vs N separate histograms:
//! - Cheaper when the tag cardinality is small but the value range
//!   is wide (one bucket array shared across tags).
//! - More expensive per-tag percentile read (must walk all buckets).
//! - Constant overhead per untagged write is one extra map lookup.

use crate::{index_of, value_from_index};
use std::collections::HashMap;

/// Histogram with per-bucket tag-keyed sub-counters.
pub struct TaggedHdrHistogram {
    sub_count_bits: u32,
    /// Outer: bucket index. Inner: tag -> count.
    buckets: Vec<HashMap<u8, u64>>,
    total: u64,
    high_index: usize,
    /// Per-tag running totals so `count_for_tag()` is O(1).
    per_tag_total: HashMap<u8, u64>,
}

impl TaggedHdrHistogram {
    /// New tagged histogram with the given significant-digit precision.
    pub fn new(significant_digits: u32) -> Self {
        let sig = significant_digits.clamp(1, 5);
        let target = 2u32 * 10u32.pow(sig);
        let sub_count_bits = (32 - target.leading_zeros()).max(1);
        let sub_count = 1u32 << sub_count_bits;
        Self {
            sub_count_bits,
            buckets: (0..sub_count).map(|_| HashMap::new()).collect(),
            total: 0,
            high_index: 0,
            per_tag_total: HashMap::new(),
        }
    }

    /// Record a value tagged with the given byte.
    pub fn record(&mut self, value: u64, tag: u8) {
        let idx = index_of(value, self.sub_count_bits) as usize;
        if idx >= self.buckets.len() {
            self.buckets.resize_with(idx + 1, HashMap::new);
        }
        *self.buckets[idx].entry(tag).or_insert(0) += 1;
        self.total += 1;
        if idx > self.high_index {
            self.high_index = idx;
        }
        *self.per_tag_total.entry(tag).or_insert(0) += 1;
    }

    pub fn count(&self) -> u64 {
        self.total
    }

    pub fn count_for_tag(&self, tag: u8) -> u64 {
        self.per_tag_total.get(&tag).copied().unwrap_or(0)
    }

    pub fn max(&self) -> u64 {
        if self.total == 0 {
            return 0;
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    /// Quantile across all tags (matches the base histogram's
    /// behaviour).
    pub fn value_at_percentile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * self.total as f64) as u64).max(1);
        let mut cum = 0u64;
        let end = (self.high_index + 1).min(self.buckets.len());
        for i in 0..end {
            let bucket_total: u64 = self.buckets[i].values().sum();
            cum += bucket_total;
            if cum >= target {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    /// Quantile restricted to one tag.
    pub fn value_at_percentile_for_tag(&self, q: f64, tag: u8) -> u64 {
        let tag_total = self.count_for_tag(tag);
        if tag_total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * tag_total as f64) as u64).max(1);
        let mut cum = 0u64;
        let end = (self.high_index + 1).min(self.buckets.len());
        for i in 0..end {
            if let Some(&c) = self.buckets[i].get(&tag) {
                cum += c;
                if cum >= target {
                    return value_from_index(i, self.sub_count_bits);
                }
            }
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    /// List of tags seen in any recording, in unspecified order.
    pub fn tags(&self) -> Vec<u8> {
        self.per_tag_total.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_zero() {
        let h = TaggedHdrHistogram::new(3);
        assert_eq!(h.count(), 0);
        assert_eq!(h.count_for_tag(1), 0);
        assert_eq!(h.value_at_percentile(0.99), 0);
        assert_eq!(h.value_at_percentile_for_tag(0.99, 1), 0);
        assert_eq!(h.max(), 0);
    }

    #[test]
    fn records_tagged_counts() {
        let mut h = TaggedHdrHistogram::new(3);
        for v in 1u64..=10 {
            h.record(v, 1);
        }
        for v in 100u64..=109 {
            h.record(v, 2);
        }
        assert_eq!(h.count(), 20);
        assert_eq!(h.count_for_tag(1), 10);
        assert_eq!(h.count_for_tag(2), 10);
        assert_eq!(h.count_for_tag(3), 0);
    }

    #[test]
    fn per_tag_percentiles_are_separate() {
        let mut h = TaggedHdrHistogram::new(3);
        // Tag 1: small values.
        for v in 1u64..=1000 {
            h.record(v, 1);
        }
        // Tag 2: big values.
        for v in 10_000u64..=11_000 {
            h.record(v, 2);
        }
        let p99_a = h.value_at_percentile_for_tag(0.99, 1);
        let p99_b = h.value_at_percentile_for_tag(0.99, 2);
        assert!(p99_a < 1100, "tag 1 p99 small: {p99_a}");
        assert!(p99_b >= 10_000, "tag 2 p99 large: {p99_b}");
    }

    #[test]
    fn aggregate_percentile_spans_all_tags() {
        let mut h = TaggedHdrHistogram::new(3);
        for v in 1u64..=500 {
            h.record(v, 1);
        }
        for v in 501u64..=1000 {
            h.record(v, 2);
        }
        assert_eq!(h.count(), 1000);
        let p50 = h.value_at_percentile(0.5);
        assert!((450..=550).contains(&p50), "aggregate p50={p50}");
    }

    #[test]
    fn tags_listing_returns_unique() {
        let mut h = TaggedHdrHistogram::new(3);
        h.record(10, 1);
        h.record(20, 2);
        h.record(30, 1);
        h.record(40, 3);
        let mut tags = h.tags();
        tags.sort();
        assert_eq!(tags, vec![1u8, 2, 3]);
    }

    #[test]
    fn unknown_tag_percentile_is_zero() {
        let mut h = TaggedHdrHistogram::new(3);
        h.record(50, 1);
        assert_eq!(h.value_at_percentile_for_tag(0.5, 99), 0);
    }

    #[test]
    fn many_tags_per_bucket() {
        let mut h = TaggedHdrHistogram::new(3);
        // All records land at value=50 - same bucket - across 10 tags.
        for tag in 0u8..10 {
            for _ in 0..100 {
                h.record(50, tag);
            }
        }
        assert_eq!(h.count(), 1000);
        for tag in 0u8..10 {
            assert_eq!(h.count_for_tag(tag), 100);
            assert!(h.value_at_percentile_for_tag(0.99, tag) > 0);
        }
    }
}
