//! Pluggable backend for cross-process rate limiting state.
//!
//! The `Backend` trait abstracts the atomic INCR + EXPIRE primitive
//! shared by Redis / Memcached / DynamoDB / any other store that can
//! atomically bump a counter scoped to a fixed window. Real distributed
//! backends are downstream user concerns; we ship a real
//! `InMemoryBackend` so the trait is exercised + the in-process shape
//! matches what a Redis-style impl would do over the wire.
//!
//! Algorithm: fixed-window counters. Each `(key, window_start_ns)`
//! pair holds a count; `incr` returns the new count after bump, the
//! backend collects garbage on window roll. Simpler than sliding-
//! window + good enough for the cross-process case (the network
//! round-trip cost dwarfs the windowing imprecision).

use std::collections::HashMap;
use std::sync::Mutex;

use super::clock::{Clock, SystemClock};

/// Cross-process state backend. Implementations bump a counter for
/// `(key, window_start)` and return the new value after the bump.
pub trait Backend: Send + Sync {
    /// Increment the counter at `key` for `window_start_ns`. Returns
    /// the new count after the bump. Must be atomic across concurrent
    /// callers.
    fn incr(&self, key: &str, window_start_ns: u64, ttl_ns: u64) -> u64;

    /// Read the current counter without bumping. Returns 0 if the
    /// (key, window) pair is unknown or expired.
    fn read(&self, key: &str, window_start_ns: u64) -> u64;
}

/// Real in-process backend. Holds counters in a `HashMap<(key, window), u64>`
/// guarded by a `Mutex`. Garbage-collects expired windows opportunistically
/// on each `incr` call.
pub struct InMemoryBackend {
    inner: Mutex<Inner>,
}

struct Inner {
    counters: HashMap<(String, u64), Cell>,
}

#[derive(Clone, Copy)]
struct Cell {
    count: u64,
    expires_ns: u64,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                counters: HashMap::new(),
            }),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for InMemoryBackend {
    fn incr(&self, key: &str, window_start_ns: u64, ttl_ns: u64) -> u64 {
        let mut g = self.inner.lock().unwrap();
        // Opportunistic GC: drop windows whose TTL has expired by
        // wall-clock-of-callsite. We accept this is imprecise vs
        // actual now-ns; for the cross-process case the caller's
        // clock IS the source of truth.
        let now = window_start_ns;
        g.counters.retain(|_, c| c.expires_ns > now);
        let entry = g
            .counters
            .entry((key.to_string(), window_start_ns))
            .or_insert(Cell {
                count: 0,
                expires_ns: window_start_ns.saturating_add(ttl_ns),
            });
        entry.count = entry.count.saturating_add(1);
        entry.count
    }

    fn read(&self, key: &str, window_start_ns: u64) -> u64 {
        let g = self.inner.lock().unwrap();
        g.counters
            .get(&(key.to_string(), window_start_ns))
            .map(|c| c.count)
            .unwrap_or(0)
    }
}

/// Rate limiter backed by a pluggable `Backend`. Fixed-window
/// algorithm: per `(key, window_size_ns)`, allow at most `limit`
/// requests.
pub struct DistributedLimiter {
    backend: Box<dyn Backend>,
    clock: Box<dyn Clock>,
    limit: u64,
    window_ns: u64,
}

impl DistributedLimiter {
    pub fn new(backend: Box<dyn Backend>, limit: u64, window_ns: u64) -> Self {
        Self::with_clock(backend, limit, window_ns, Box::new(SystemClock::new()))
    }

    pub fn with_clock(
        backend: Box<dyn Backend>,
        limit: u64,
        window_ns: u64,
        clock: Box<dyn Clock>,
    ) -> Self {
        Self {
            backend,
            clock,
            limit: limit.max(1),
            window_ns: window_ns.max(1),
        }
    }

    /// Try to acquire one permit on `key`. Returns true if the post-bump
    /// counter is within `limit`. The bump always happens (mirroring
    /// the Redis INCR + EXPIRE shape) so contention races resolve
    /// monotonically.
    pub fn try_acquire(&self, key: &str) -> bool {
        let now = self.clock.now_ns();
        let window_start = now - (now % self.window_ns);
        let count = self.backend.incr(key, window_start, self.window_ns);
        count <= self.limit
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn window_ns(&self) -> u64 {
        self.window_ns
    }
}

#[cfg(test)]
#[path = "distributed_backend_tests.rs"]
mod tests;
