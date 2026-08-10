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
    /// is only bumped via [`Self::record_enqueue_fail`] from a bounded
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

    /// Borrow the next value without consuming it. Does not touch the
    /// counters: a peek is not a dequeue.
    pub fn peek(&mut self) -> Option<&T> {
        self.inner.peek()
    }

    /// See [`MpscQueue::is_empty`].
    pub fn is_empty(&mut self) -> bool {
        self.inner.is_empty()
    }

    /// See [`MpscQueue::len`]. O(n) in the backlog.
    pub fn len(&mut self) -> usize {
        self.inner.len()
    }

    /// Drain everything reachable and return the count. The drained items
    /// count as successful dequeues, so a cleared backlog still shows up in
    /// the snapshot rather than vanishing from the totals.
    pub fn clear(&mut self) -> usize {
        let n = self.inner.clear();
        self.dequeue_ok.fetch_add(n as u64, Ordering::Relaxed);
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
#[path = "metrics_tests.rs"]
mod tests;
