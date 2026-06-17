//! Per-instance metrics wrapper.
//!
//! Wraps a base `Producer` / `Consumer` pair behind `try_push` / `try_pop`
//! and records:
//!
//! - `enqueue_success`, `enqueue_fail` (ring full)
//! - `dequeue_success`, `dequeue_fail` (ring empty)
//! - `max_depth_observed` (gauge; high-water mark of in-flight items)
//! - `cas_retries` (only set by the mpmc-disruptor path; SPSC never CAS-retries)
//!
//! Counter overhead is one `fetch_add` per op - measurable on a hot loop, so
//! reach for this when you're explicitly capturing operational stats, not
//! when you need the absolute lowest latency.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Consumer, Producer};

/// Shared counter set. Hold an `Arc<RingMetrics>` across producer + consumer
/// so the snapshot reflects both sides.
pub struct RingMetrics {
    enqueue_success: AtomicU64,
    enqueue_fail: AtomicU64,
    dequeue_success: AtomicU64,
    dequeue_fail: AtomicU64,
    max_depth_observed: AtomicU64,
    cas_retries: AtomicU64,
}

impl Default for RingMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl RingMetrics {
    pub fn new() -> Self {
        Self {
            enqueue_success: AtomicU64::new(0),
            enqueue_fail: AtomicU64::new(0),
            dequeue_success: AtomicU64::new(0),
            dequeue_fail: AtomicU64::new(0),
            max_depth_observed: AtomicU64::new(0),
            cas_retries: AtomicU64::new(0),
        }
    }

    /// Bump the CAS-retry counter. Called by the mpmc-disruptor path; the
    /// SPSC paths never retry under the wait-free invariant.
    pub fn record_cas_retry(&self) {
        self.cas_retries.fetch_add(1, Ordering::Relaxed);
    }

    /// Capture a point-in-time snapshot of every counter. Consistent enough
    /// for monitoring; not a transactional read across all six values.
    pub fn snapshot(&self) -> RingMetricsSnapshot {
        RingMetricsSnapshot {
            enqueue_success: self.enqueue_success.load(Ordering::Relaxed),
            enqueue_fail: self.enqueue_fail.load(Ordering::Relaxed),
            dequeue_success: self.dequeue_success.load(Ordering::Relaxed),
            dequeue_fail: self.dequeue_fail.load(Ordering::Relaxed),
            max_depth_observed: self.max_depth_observed.load(Ordering::Relaxed),
            cas_retries: self.cas_retries.load(Ordering::Relaxed),
        }
    }

    fn observe_depth(&self, d: u64) {
        let mut cur = self.max_depth_observed.load(Ordering::Relaxed);
        while d > cur {
            match self.max_depth_observed.compare_exchange_weak(
                cur,
                d,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(latest) => cur = latest,
            }
        }
    }
}

/// Snapshot value-type returned by `RingMetrics::snapshot`. `Clone + Copy` so
/// callers can stash + compare across calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingMetricsSnapshot {
    pub enqueue_success: u64,
    pub enqueue_fail: u64,
    pub dequeue_success: u64,
    pub dequeue_fail: u64,
    pub max_depth_observed: u64,
    pub cas_retries: u64,
}

/// Helper that constructs a metrics-wrapped (producer, consumer, metrics)
/// triple from a base SPSC pair.
pub struct InstrumentedSpsc;

impl InstrumentedSpsc {
    /// Wrap an existing `(Producer, Consumer)` with counters. Returns the
    /// instrumented sides and an `Arc<RingMetrics>` shared across both.
    pub fn wrap<T>(
        producer: Producer<T>,
        consumer: Consumer<T>,
    ) -> (
        InstrumentedProducer<T>,
        InstrumentedConsumer<T>,
        Arc<RingMetrics>,
    ) {
        let metrics = Arc::new(RingMetrics::new());
        let p = InstrumentedProducer {
            inner: producer,
            metrics: metrics.clone(),
            local_depth: 0,
        };
        let c = InstrumentedConsumer {
            inner: consumer,
            metrics: metrics.clone(),
        };
        (p, c, metrics)
    }
}

pub struct InstrumentedProducer<T> {
    inner: Producer<T>,
    metrics: Arc<RingMetrics>,
    /// Producer-local view of in-flight count; +1 on success, no syscall.
    local_depth: u64,
}

impl<T> InstrumentedProducer<T> {
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        match self.inner.try_push(value) {
            Ok(()) => {
                self.metrics.enqueue_success.fetch_add(1, Ordering::Relaxed);
                self.local_depth += 1;
                self.metrics.observe_depth(self.local_depth);
                Ok(())
            }
            Err(v) => {
                self.metrics.enqueue_fail.fetch_add(1, Ordering::Relaxed);
                Err(v)
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

pub struct InstrumentedConsumer<T> {
    inner: Consumer<T>,
    metrics: Arc<RingMetrics>,
}

impl<T> InstrumentedConsumer<T> {
    pub fn try_pop(&mut self) -> Option<T> {
        match self.inner.try_pop() {
            Some(v) => {
                self.metrics.dequeue_success.fetch_add(1, Ordering::Relaxed);
                Some(v)
            }
            None => {
                self.metrics.dequeue_fail.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::SpscRingBuffer;

    #[test]
    fn counts_enqueue_success_and_fail() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(2);
        let (mut tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
        tx.try_push(1).unwrap();
        tx.try_push(2).unwrap();
        assert!(tx.try_push(3).is_err());
        let s = m.snapshot();
        assert_eq!(s.enqueue_success, 2);
        assert_eq!(s.enqueue_fail, 1);
    }

    #[test]
    fn counts_dequeue_success_and_fail() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let (mut tx, mut rx, m) = InstrumentedSpsc::wrap(tx, rx);
        tx.try_push(7).unwrap();
        assert_eq!(rx.try_pop(), Some(7));
        assert_eq!(rx.try_pop(), None);
        assert_eq!(rx.try_pop(), None);
        let s = m.snapshot();
        assert_eq!(s.dequeue_success, 1);
        assert_eq!(s.dequeue_fail, 2);
    }

    #[test]
    fn tracks_max_depth_observed() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let (mut tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
        for i in 0..4u32 {
            tx.try_push(i).unwrap();
        }
        let s = m.snapshot();
        assert_eq!(s.max_depth_observed, 4);
    }

    #[test]
    fn snapshot_returns_consistent_zeros_initially() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let (_tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
        let s = m.snapshot();
        assert_eq!(
            s,
            RingMetricsSnapshot {
                enqueue_success: 0,
                enqueue_fail: 0,
                dequeue_success: 0,
                dequeue_fail: 0,
                max_depth_observed: 0,
                cas_retries: 0,
            }
        );
    }

    #[test]
    fn cas_retry_counter_is_writeable() {
        let m = RingMetrics::new();
        for _ in 0..7 {
            m.record_cas_retry();
        }
        assert_eq!(m.snapshot().cas_retries, 7);
    }

    #[test]
    fn metrics_observed_across_threads() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(64);
        let (mut tx, mut rx, m) = InstrumentedSpsc::wrap(tx, rx);
        let n = 10_000u64;
        let producer = thread::spawn(move || {
            for i in 0..n {
                while tx.try_push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });
        let consumer = thread::spawn(move || {
            let mut got = 0u64;
            while got < n {
                if rx.try_pop().is_some() {
                    got += 1;
                }
            }
        });
        producer.join().unwrap();
        consumer.join().unwrap();
        let s = m.snapshot();
        assert_eq!(s.enqueue_success, n);
        assert_eq!(s.dequeue_success, n);
    }
}
