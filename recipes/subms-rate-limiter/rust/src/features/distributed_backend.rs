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
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::features::clock::TestClock;

    struct ArcClock(Arc<TestClock>);
    impl Clock for ArcClock {
        fn now_ns(&self) -> u64 {
            self.0.now_ns()
        }
    }

    fn make(limit: u64, window_ms: u64) -> (DistributedLimiter, Arc<TestClock>) {
        let clk = Arc::new(TestClock::new());
        let c = clk.clone();
        let backend: Box<dyn Backend> = Box::new(InMemoryBackend::new());
        let limiter = DistributedLimiter::with_clock(
            backend,
            limit,
            window_ms.saturating_mul(1_000_000),
            Box::new(ArcClock(c)),
        );
        (limiter, clk)
    }

    #[test]
    fn first_burst_grants_up_to_limit() {
        let (lim, _clk) = make(5, 1000);
        for _ in 0..5 {
            assert!(lim.try_acquire("user-1"));
        }
        assert!(!lim.try_acquire("user-1"), "limit reached");
    }

    #[test]
    fn window_roll_resets_count() {
        let (lim, clk) = make(3, 100); // 100 ms windows
        for _ in 0..3 {
            assert!(lim.try_acquire("k"));
        }
        assert!(!lim.try_acquire("k"));
        // Advance past the window boundary.
        clk.advance_ms(150);
        for _ in 0..3 {
            assert!(lim.try_acquire("k"), "new window should grant");
        }
        assert!(!lim.try_acquire("k"));
    }

    #[test]
    fn keys_are_isolated() {
        let (lim, _clk) = make(2, 1000);
        for _ in 0..2 {
            assert!(lim.try_acquire("a"));
        }
        assert!(!lim.try_acquire("a"));
        // Other key starts fresh.
        for _ in 0..2 {
            assert!(lim.try_acquire("b"));
        }
    }

    #[test]
    fn backend_swap_preserves_contract() {
        // Construct two limiters sharing the same in-memory backend
        // (via the Backend trait object). Both bump the same counter
        // when used on the same key + window.
        let clk = Arc::new(TestClock::new());
        let shared: Arc<InMemoryBackend> = Arc::new(InMemoryBackend::new());

        // Adapter Box that forwards to the shared Arc.
        struct SharedBackend(Arc<InMemoryBackend>);
        impl Backend for SharedBackend {
            fn incr(&self, key: &str, ws: u64, ttl: u64) -> u64 {
                self.0.incr(key, ws, ttl)
            }
            fn read(&self, key: &str, ws: u64) -> u64 {
                self.0.read(key, ws)
            }
        }

        let l1 = DistributedLimiter::with_clock(
            Box::new(SharedBackend(shared.clone())),
            4,
            1_000_000_000,
            Box::new(ArcClock(clk.clone())),
        );
        let l2 = DistributedLimiter::with_clock(
            Box::new(SharedBackend(shared.clone())),
            4,
            1_000_000_000,
            Box::new(ArcClock(clk.clone())),
        );
        assert!(l1.try_acquire("k"));
        assert!(l1.try_acquire("k"));
        assert!(l2.try_acquire("k"));
        assert!(l2.try_acquire("k"));
        // 4 used across both limiters via the shared backend.
        assert!(!l1.try_acquire("k"));
        assert!(!l2.try_acquire("k"));
    }

    #[test]
    fn read_without_bump_is_observation() {
        let backend = InMemoryBackend::new();
        // No incr yet -> reads 0.
        assert_eq!(backend.read("k", 0), 0);
        let _ = backend.incr("k", 0, 1_000_000);
        let _ = backend.incr("k", 0, 1_000_000);
        assert_eq!(backend.read("k", 0), 2);
    }

    #[test]
    fn limit_and_window_accessors() {
        let (lim, _clk) = make(42, 250);
        assert_eq!(lim.limit(), 42);
        assert_eq!(lim.window_ns(), 250_000_000);
    }
}
