//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base order-session throttle plus one test per optional feature, gated the
//! same way. Std-only, not harness-gated.

use subms_rate_limiter::{Acquire, RateLimiter};

#[test]
fn base_order_session_bursts_then_throttles() {
    let session = RateLimiter::new(100.0, 5);
    let mut granted = 0usize;
    for _ in 0..8 {
        if session.try_acquire() {
            granted += 1;
        }
    }
    assert_eq!(granted, 5, "the burst allowance admits exactly 5 orders");

    match session.try_acquire_with_retry() {
        Acquire::Ok => panic!("a saturated session must not grant"),
        Acquire::Retry(wait) => assert!(!wait.is_zero(), "a throttled caller gets a positive wait"),
    }
}

#[cfg(feature = "token-bucket")]
#[test]
fn token_bucket_weighted_batch_is_atomic() {
    use std::sync::Arc;

    use subms_rate_limiter::{TestClock, TokenBucket};

    let clock = Arc::new(TestClock::new());
    let budget = TokenBucket::with_clock(10, 5.0, Box::new(SharedClock(clock.clone())));

    assert!(budget.try_acquire(1));
    assert!(budget.try_acquire(5));
    assert_eq!(budget.available(), 4);
    assert!(!budget.try_acquire(5), "an over-budget batch is rejected");
    assert_eq!(budget.available(), 4, "a rejected batch spends nothing");

    clock.advance_ms(1_000);
    assert!(budget.try_acquire(5), "refilled budget admits the batch");
}

#[cfg(feature = "hierarchical")]
#[test]
fn hierarchical_parent_caps_the_aggregate() {
    use std::sync::Arc;

    use subms_rate_limiter::{HierarchicalLimiter, TestClock};

    let clock = Arc::new(TestClock::new());
    let c = clock.clone();
    let desk =
        HierarchicalLimiter::with_clock_fn(5, 0.0, 2, 10, 0.0, || Box::new(SharedClock(c.clone())));

    let mut sent = 0usize;
    for round in 0..10 {
        if desk.try_acquire(round % 2, 1) {
            sent += 1;
        }
    }
    assert_eq!(sent, 5, "the parent caps the desk aggregate at 5");
}

#[cfg(feature = "distributed-backend")]
#[test]
fn distributed_quota_holds_across_routers() {
    use std::sync::Arc;

    use subms_rate_limiter::{DistributedLimiter, InMemoryBackend, TestClock};

    let clock = Arc::new(TestClock::new());
    let shared = Arc::new(InMemoryBackend::new());
    let window_ns = 1_000_000_000u64;

    let router_a = DistributedLimiter::with_clock(
        Box::new(SharedBackend(shared.clone())),
        5,
        window_ns,
        Box::new(SharedClock(clock.clone())),
    );
    let router_b = DistributedLimiter::with_clock(
        Box::new(SharedBackend(shared.clone())),
        5,
        window_ns,
        Box::new(SharedClock(clock.clone())),
    );

    let mut admitted = 0usize;
    for round in 0..8 {
        let router = if round % 2 == 0 { &router_a } else { &router_b };
        if router.try_acquire("acct-42") {
            admitted += 1;
        }
    }
    assert_eq!(admitted, 5, "the shared quota holds across both routers");
}

#[cfg(feature = "metrics")]
#[test]
fn metrics_snapshot_counts_grants_rejects_and_refills() {
    use std::sync::Arc;

    use subms_rate_limiter::{MeteredTokenBucket, TestClock};

    let clock = Arc::new(TestClock::new());
    let feed = MeteredTokenBucket::with_clock(5, 100.0, Box::new(SharedClock(clock.clone())));

    for _ in 0..8 {
        feed.try_acquire(1);
    }
    let s = feed.snapshot();
    assert_eq!(s.granted, 5);
    assert_eq!(s.rejected, 3);
    assert_eq!(s.refills, 0, "no refill while the clock is still");

    clock.advance_ms(100);
    assert!(feed.try_acquire(1));
    let s2 = feed.snapshot();
    assert_eq!(s2.granted, 6);
    assert!(s2.refills >= 1, "a refill step was observed");
}

#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
))]
struct SharedClock(std::sync::Arc<subms_rate_limiter::TestClock>);

#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
))]
impl subms_rate_limiter::Clock for SharedClock {
    fn now_ns(&self) -> u64 {
        self.0.now_ns()
    }
}

#[cfg(feature = "distributed-backend")]
struct SharedBackend(std::sync::Arc<subms_rate_limiter::InMemoryBackend>);

#[cfg(feature = "distributed-backend")]
impl subms_rate_limiter::Backend for SharedBackend {
    fn incr(&self, key: &str, window_start_ns: u64, ttl_ns: u64) -> u64 {
        self.0.incr(key, window_start_ns, ttl_ns)
    }
    fn read(&self, key: &str, window_start_ns: u64) -> u64 {
        self.0.read(key, window_start_ns)
    }
}
