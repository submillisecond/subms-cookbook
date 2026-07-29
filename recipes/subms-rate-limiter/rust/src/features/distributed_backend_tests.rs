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
