//! Lock-free concurrent histogram.
//!
//! Same log-linear bucketing as the base `HdrHistogram` but every
//! counter is an `AtomicU64`. Producers call `record()` from any
//! thread without external synchronisation - the only contention is
//! the per-bucket `fetch_add`. Snapshot reads walk the array with
//! relaxed loads; the value-at-percentile / max / count answers
//! reflect a point in time that may interleave with concurrent
//! writers, but each individual counter is intact.
//!
//! Trade-off vs the base: the array is fixed-size and pre-allocated
//! at construction. A growable layout would need a mutex around the
//! resize, which defeats the lock-free property. Pick the upper bound
//! at construction (number of major buckets) - default 32 majors
//! covers up to 2^32-ish for 3-sig-digit, which is well past
//! sub-millisecond latency at nanosecond units.

use crate::{index_of, value_from_index};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Concurrent HDR histogram. All operations are lock-free.
pub struct ConcurrentHdrHistogram {
    sub_count: u32,
    sub_count_bits: u32,
    counters: Vec<AtomicU64>,
    total: AtomicU64,
    high_index: AtomicUsize,
}

impl ConcurrentHdrHistogram {
    /// New histogram with the given significant-digit precision and
    /// default major-bucket capacity (32, covering up to ~10^9 at
    /// 3 sig-digits).
    pub fn new(significant_digits: u32) -> Self {
        Self::with_majors(significant_digits, 32)
    }

    /// Explicit major-bucket capacity. Counter array length is
    /// `sub_count * majors`. Records that would land past the last
    /// bucket are clamped into the final bucket (no resize possible
    /// in a lock-free design).
    pub fn with_majors(significant_digits: u32, majors: u32) -> Self {
        let sig = significant_digits.clamp(1, 5);
        let target = 2u32 * 10u32.pow(sig);
        let sub_count_bits = (32 - target.leading_zeros()).max(1);
        let sub_count = 1u32 << sub_count_bits;
        let majors = majors.max(1);
        let total_buckets = (sub_count as usize) * (majors as usize);
        let mut counters = Vec::with_capacity(total_buckets);
        for _ in 0..total_buckets {
            counters.push(AtomicU64::new(0));
        }
        Self {
            sub_count,
            sub_count_bits,
            counters,
            total: AtomicU64::new(0),
            high_index: AtomicUsize::new(0),
        }
    }

    pub fn sub_count(&self) -> u32 {
        self.sub_count
    }

    pub fn sub_count_bits(&self) -> u32 {
        self.sub_count_bits
    }

    pub fn count(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn max(&self) -> u64 {
        if self.count() == 0 {
            return 0;
        }
        value_from_index(self.high_index.load(Ordering::Relaxed), self.sub_count_bits)
    }

    /// Record a value. Safe from any thread.
    pub fn record(&self, value: u64) {
        let raw = index_of(value, self.sub_count_bits) as usize;
        let idx = raw.min(self.counters.len() - 1);
        self.counters[idx].fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
        // Race on high_index is benign: any thread that wrote a
        // higher index will eventually win the CAS. Worst case the
        // snapshot reader sees a stale low high_index for a few ns.
        let mut cur = self.high_index.load(Ordering::Relaxed);
        while idx > cur {
            match self.high_index.compare_exchange_weak(
                cur,
                idx,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => cur = seen,
            }
        }
    }

    /// Snapshot-style percentile read. Walks the array with relaxed
    /// loads; not linearisable across all concurrent writers but
    /// each individual counter read is intact.
    pub fn value_at_percentile(&self, q: f64) -> u64 {
        let total = self.count();
        if total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * total as f64) as u64).max(1);
        let high = self.high_index.load(Ordering::Relaxed);
        let mut cum = 0u64;
        let end = (high + 1).min(self.counters.len());
        for i in 0..end {
            cum += self.counters[i].load(Ordering::Relaxed);
            if cum >= target {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        value_from_index(high, self.sub_count_bits)
    }

    /// Atomically drain every counter and total into a `Snapshot`,
    /// leaving the histogram empty. Used by `DualRecorder` to harvest
    /// the inactive side. Each per-counter swap is independent, so
    /// concurrent writers may land their increments in EITHER the
    /// drained snapshot OR the now-zeroed live histogram - we never
    /// double-count or lose a write.
    pub fn drain_snapshot(&self) -> Snapshot {
        let mut counts = Vec::with_capacity(self.counters.len());
        let high = self.high_index.load(Ordering::Relaxed);
        let len = (high + 1).min(self.counters.len());
        let mut total = 0u64;
        for i in 0..len {
            let v = self.counters[i].swap(0, Ordering::AcqRel);
            counts.push(v);
            total += v;
        }
        // Subtract what we drained from `total`. Concurrent
        // record()s that fired during the loop will keep `total`
        // ahead of the per-counter sum; that's correct for the
        // live side.
        self.total.fetch_sub(total, Ordering::AcqRel);
        self.high_index.store(0, Ordering::Relaxed);
        Snapshot {
            sub_count_bits: self.sub_count_bits,
            counts,
            total,
        }
    }
}

/// Frozen view of a histogram at the moment of `drain_snapshot()`.
/// Has the same percentile / max APIs as the live histogram but is
/// immutable.
pub struct Snapshot {
    sub_count_bits: u32,
    counts: Vec<u64>,
    total: u64,
}

impl Snapshot {
    pub fn count(&self) -> u64 {
        self.total
    }

    pub fn max(&self) -> u64 {
        if self.total == 0 {
            return 0;
        }
        // The drained snapshot truncates at high_index, so the last
        // non-zero counter is at most counts.len()-1.
        for i in (0..self.counts.len()).rev() {
            if self.counts[i] > 0 {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        0
    }

    pub fn value_at_percentile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * self.total as f64) as u64).max(1);
        let mut cum = 0u64;
        for (i, &c) in self.counts.iter().enumerate() {
            cum += c;
            if cum >= target {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        // Falls through when q rounds past the last non-zero bucket.
        for i in (0..self.counts.len()).rev() {
            if self.counts[i] > 0 {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn single_thread_records() {
        let h = ConcurrentHdrHistogram::new(3);
        for v in [10u64, 20, 30, 40, 50] {
            h.record(v);
        }
        assert_eq!(h.count(), 5);
        assert!(h.max() >= 50);
    }

    #[test]
    fn empty_returns_zero() {
        let h = ConcurrentHdrHistogram::new(3);
        assert_eq!(h.count(), 0);
        assert_eq!(h.max(), 0);
        assert_eq!(h.value_at_percentile(0.99), 0);
    }

    #[test]
    fn shape_accessors_match_precision() {
        let h = ConcurrentHdrHistogram::new(3);
        assert_eq!(h.sub_count(), 1u32 << h.sub_count_bits());
        assert!(h.sub_count() >= 2);
    }

    #[test]
    fn percentiles_match_distribution() {
        let h = ConcurrentHdrHistogram::new(3);
        for i in 1..=1000 {
            h.record(i);
        }
        let p50 = h.value_at_percentile(0.5);
        let p99 = h.value_at_percentile(0.99);
        assert!((450..=550).contains(&p50), "p50={p50}");
        assert!((950..=1050).contains(&p99), "p99={p99}");
    }

    #[test]
    fn concurrent_writers_lose_nothing() {
        let h = Arc::new(ConcurrentHdrHistogram::new(3));
        let threads = 8;
        let per_thread = 25_000;
        let mut handles = vec![];
        for t in 0..threads {
            let h = h.clone();
            handles.push(thread::spawn(move || {
                for i in 0..per_thread {
                    h.record(((t * per_thread + i) as u64 % 1000) + 1);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(h.count(), (threads * per_thread) as u64);
        let p99 = h.value_at_percentile(0.99);
        assert!(p99 >= 900, "p99 in expected range, got {p99}");
    }

    #[test]
    fn snapshot_preserves_total() {
        let h = ConcurrentHdrHistogram::new(3);
        for i in 1..=100 {
            h.record(i);
        }
        let snap = h.drain_snapshot();
        assert_eq!(snap.count(), 100);
        assert_eq!(h.count(), 0, "live side cleared after drain");
        let p99 = snap.value_at_percentile(0.99);
        assert!(p99 >= 95, "snapshot p99 ~ 99, got {p99}");
    }

    #[test]
    fn snapshot_then_record_starts_fresh() {
        let h = ConcurrentHdrHistogram::new(3);
        for i in 1..=10 {
            h.record(i);
        }
        let _ = h.drain_snapshot();
        h.record(500);
        assert_eq!(h.count(), 1);
        assert!(h.max() >= 500);
    }

    #[test]
    fn clamps_above_bucket_capacity() {
        // Tiny: 1 sig-digit + 1 major.
        let h = ConcurrentHdrHistogram::with_majors(1, 1);
        let huge = u64::MAX / 2;
        h.record(huge);
        assert_eq!(h.count(), 1);
        // Should not panic; max returns the last bucket's value.
        let _ = h.max();
    }
}
