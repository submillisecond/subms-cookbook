//! Injected monotonic clock. Production wires `SystemClock`
//! (`Instant::elapsed` under the hood); tests wire `TestClock` to
//! advance time deterministically without wall-clock sleeps.

use std::sync::Mutex;
use std::time::Instant;

/// Monotonic ns-precision clock. Implementations must be thread-safe
/// and must never go backwards.
pub trait Clock: Send + Sync {
    /// Nanoseconds since the clock's origin. Monotonic non-decreasing.
    fn now_ns(&self) -> u64;
}

/// Wall-clock implementation. Origin is the moment the instance is
/// constructed; `now_ns` returns `Instant::elapsed` against that origin.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ns(&self) -> u64 {
        self.origin.elapsed().as_nanos() as u64
    }
}

/// Deterministic clock for tests. `advance(ns)` moves the clock
/// forward; `now_ns()` reads the current value.
pub struct TestClock {
    now: Mutex<u64>,
}

impl TestClock {
    pub fn new() -> Self {
        Self { now: Mutex::new(0) }
    }

    pub fn with_start(start_ns: u64) -> Self {
        Self {
            now: Mutex::new(start_ns),
        }
    }

    pub fn advance(&self, ns: u64) {
        let mut g = self.now.lock().unwrap();
        *g = g.saturating_add(ns);
    }

    pub fn advance_ms(&self, ms: u64) {
        self.advance(ms.saturating_mul(1_000_000));
    }
}

impl Default for TestClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for TestClock {
    fn now_ns(&self) -> u64 {
        *self.now.lock().unwrap()
    }
}
