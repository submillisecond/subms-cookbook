//! Per-instance metrics wrapper.
//!
//! Wraps the base [`MpscQueue`] with relaxed atomic counters for
//! enqueue success/fail, dequeue success/fail, total batch items
//! drained, and CAS retries. Counters are relaxed because they're
//! advisory diagnostics, not ordering primitives.
//!
//! All counters are zero-cost when the wrapper isn't used; the
//! feature flag keeps them out of the base build.

use crate::{MpscQueue, PopResult};
use std::sync::atomic::{AtomicU64, Ordering};

/// Wrapping queue that tracks per-instance counters.
pub struct MetricsMpscQueue<T> {
    inner: MpscQueue<T>,
    enqueue_ok: AtomicU64,
    enqueue_fail: AtomicU64,
    dequeue_ok: AtomicU64,
    dequeue_fail: AtomicU64,
    batch_items: AtomicU64,
    cas_retries: AtomicU64,
}

/// Immutable snapshot of the counters at one instant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueMetricsSnapshot {
    pub enqueue_ok: u64,
    pub enqueue_fail: u64,
    pub dequeue_ok: u64,
    pub dequeue_fail: u64,
    pub batch_items: u64,
    pub cas_retries: u64,
}

impl<T> MetricsMpscQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: MpscQueue::new(),
            enqueue_ok: AtomicU64::new(0),
            enqueue_fail: AtomicU64::new(0),
            dequeue_ok: AtomicU64::new(0),
            dequeue_fail: AtomicU64::new(0),
            batch_items: AtomicU64::new(0),
            cas_retries: AtomicU64::new(0),
        }
    }

    /// Push always succeeds for the unbounded base; the fail counter
    /// is only bumped via [`record_enqueue_fail`] from a bounded
    /// composition wrapper.
    pub fn push(&self, value: T) {
        self.inner.push(value);
        self.enqueue_ok.fetch_add(1, Ordering::Relaxed);
    }

    /// Single-consumer pop. Bumps `dequeue_ok` or `dequeue_fail`.
    pub fn try_pop(&mut self) -> PopResult<T> {
        let r = self.inner.try_pop();
        match &r {
            PopResult::Some(_) => {
                self.dequeue_ok.fetch_add(1, Ordering::Relaxed);
            }
            PopResult::Empty | PopResult::Inconsistent => {
                self.dequeue_fail.fetch_add(1, Ordering::Relaxed);
            }
        }
        r
    }

    /// Bulk drain into `out`. Returns the number drained and bumps
    /// `batch_items` by the count.
    pub fn try_pop_batch(&mut self, out: &mut [Option<T>]) -> usize {
        let mut n = 0;
        while n < out.len() {
            match self.inner.try_pop() {
                PopResult::Some(v) => {
                    out[n] = Some(v);
                    n += 1;
                    self.dequeue_ok.fetch_add(1, Ordering::Relaxed);
                }
                PopResult::Empty | PopResult::Inconsistent => {
                    self.dequeue_fail.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
        self.batch_items.fetch_add(n as u64, Ordering::Relaxed);
        n
    }

    /// External hook for callers that combine this with a bounded
    /// upstream (or any path where an enqueue can be rejected).
    pub fn record_enqueue_fail(&self) {
        self.enqueue_fail.fetch_add(1, Ordering::Relaxed);
    }

    /// External hook used by MPMC compositions to log retry counts.
    pub fn record_cas_retries(&self, n: u64) {
        if n > 0 {
            self.cas_retries.fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Atomic-load snapshot. Counters may move between loads (relaxed
    /// across atomics), so this is a point-in-time approximation.
    pub fn snapshot(&self) -> QueueMetricsSnapshot {
        QueueMetricsSnapshot {
            enqueue_ok: self.enqueue_ok.load(Ordering::Relaxed),
            enqueue_fail: self.enqueue_fail.load(Ordering::Relaxed),
            dequeue_ok: self.dequeue_ok.load(Ordering::Relaxed),
            dequeue_fail: self.dequeue_fail.load(Ordering::Relaxed),
            batch_items: self.batch_items.load(Ordering::Relaxed),
            cas_retries: self.cas_retries.load(Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero. Useful for cycle-bounded
    /// reporting.
    pub fn reset(&self) {
        self.enqueue_ok.store(0, Ordering::Relaxed);
        self.enqueue_fail.store(0, Ordering::Relaxed);
        self.dequeue_ok.store(0, Ordering::Relaxed);
        self.dequeue_fail.store(0, Ordering::Relaxed);
        self.batch_items.store(0, Ordering::Relaxed);
        self.cas_retries.store(0, Ordering::Relaxed);
    }
}

impl<T> Default for MetricsMpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pop_until_some<T>(q: &mut MetricsMpscQueue<T>) -> Option<T> {
        let start = std::time::Instant::now();
        loop {
            match q.try_pop() {
                PopResult::Some(v) => return Some(v),
                PopResult::Empty => return None,
                PopResult::Inconsistent => {
                    if start.elapsed().as_secs() > 5 {
                        return None;
                    }
                    std::hint::spin_loop();
                }
            }
        }
    }

    #[test]
    fn snapshot_default_is_zero() {
        let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
    }

    #[test]
    fn push_increments_enqueue_ok() {
        let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        for i in 0..5 {
            q.push(i);
        }
        let s = q.snapshot();
        assert_eq!(s.enqueue_ok, 5);
        assert_eq!(s.enqueue_fail, 0);
    }

    #[test]
    fn try_pop_tracks_ok_and_fail() {
        let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        q.push(1);
        q.push(2);
        let _ = pop_until_some(&mut q).unwrap();
        let _ = pop_until_some(&mut q).unwrap();
        // Now empty:
        let _ = q.try_pop(); // empty
        let s = q.snapshot();
        assert_eq!(s.dequeue_ok, 2);
        assert!(s.dequeue_fail >= 1);
    }

    #[test]
    fn batch_records_drained_items() {
        let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        for i in 0..7 {
            q.push(i);
        }
        let mut buf: Vec<Option<u32>> = (0..10).map(|_| None).collect();
        let n = q.try_pop_batch(&mut buf);
        let s = q.snapshot();
        assert_eq!(n, 7);
        assert_eq!(s.batch_items, 7);
        assert_eq!(s.dequeue_ok, 7);
    }

    #[test]
    fn record_enqueue_fail_bumps_counter() {
        let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        q.push(1);
        q.record_enqueue_fail();
        q.record_enqueue_fail();
        let s = q.snapshot();
        assert_eq!(s.enqueue_ok, 1);
        assert_eq!(s.enqueue_fail, 2);
    }

    #[test]
    fn record_cas_retries_accumulates() {
        let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        q.record_cas_retries(5);
        q.record_cas_retries(3);
        q.record_cas_retries(0); // explicit no-op
        let s = q.snapshot();
        assert_eq!(s.cas_retries, 8);
    }

    #[test]
    fn reset_clears_all_counters() {
        let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
        q.push(1);
        q.push(2);
        let _ = pop_until_some(&mut q);
        q.record_cas_retries(7);
        q.record_enqueue_fail();
        q.reset();
        assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
    }

    #[test]
    fn default_constructor_works() {
        let q: MetricsMpscQueue<u32> = MetricsMpscQueue::default();
        assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
    }
}
