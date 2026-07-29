//! Metered token bucket: wraps `TokenBucket` and tracks per-instance
//! counters - acquires granted, acquires rejected, refill events,
//! current token level - so consumers can scrape the limiter as a
//! metric source without a separate observability layer.
//!
//! Counters are `AtomicU64`; reads via `snapshot()` are wait-free.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::clock::{Clock, SystemClock};
use super::token_bucket::TokenBucket;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    /// Total `try_acquire(n)` calls that returned `true`.
    pub granted: u64,
    /// Total `try_acquire(n)` calls that returned `false`.
    pub rejected: u64,
    /// Total refill events observed (one per non-zero refill step).
    pub refills: u64,
    /// Current available tokens at snapshot time.
    pub available: u64,
}

pub struct MeteredTokenBucket {
    inner: Arc<TokenBucket>,
    granted: AtomicU64,
    rejected: AtomicU64,
    refills: AtomicU64,
    last_available: AtomicU64,
}

impl MeteredTokenBucket {
    pub fn new(capacity: u64, rate_per_sec: f64) -> Self {
        Self::with_clock(capacity, rate_per_sec, Box::new(SystemClock::new()))
    }

    pub fn with_clock(capacity: u64, rate_per_sec: f64, clock: Box<dyn Clock>) -> Self {
        let inner = Arc::new(TokenBucket::with_clock(capacity, rate_per_sec, clock));
        let avail = inner.available();
        Self {
            inner,
            granted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            refills: AtomicU64::new(0),
            last_available: AtomicU64::new(avail),
        }
    }

    pub fn try_acquire(&self, n: u64) -> bool {
        // `available()` itself triggers refill, so it gives us the
        // post-refill snapshot. Compare against the last seen value to
        // detect that a refill stepped the count upward between calls.
        let last = self.last_available.load(Ordering::Relaxed);
        let post_refill_before = self.inner.available();
        if post_refill_before > last {
            self.refills.fetch_add(1, Ordering::Relaxed);
        }
        let ok = self.inner.try_acquire(n);
        let after = self.inner.available();
        if ok {
            self.granted.fetch_add(1, Ordering::Relaxed);
        } else {
            self.rejected.fetch_add(1, Ordering::Relaxed);
        }
        self.last_available.store(after, Ordering::Relaxed);
        ok
    }

    pub fn try_acquire_one(&self) -> bool {
        self.try_acquire(1)
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            granted: self.granted.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            refills: self.refills.load(Ordering::Relaxed),
            available: self.inner.available(),
        }
    }

    pub fn capacity(&self) -> u64 {
        self.inner.capacity()
    }

    pub fn rate_per_sec(&self) -> f64 {
        self.inner.rate_per_sec()
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
