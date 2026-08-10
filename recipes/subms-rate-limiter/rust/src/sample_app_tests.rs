//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base order-session throttle plus one test per optional feature, gated the
//! same way. Std-only, not harness-gated.

use super::*;

/// The example's order tape, and the admission decisions it produces. Kept in
/// lockstep with `examples/sample_app.rs` so the page's printed output is a
/// tested artefact rather than a transcript nobody re-runs.
const TAPE: &[(u64, &str, u64)] = &[
    (0, "ESU5", 1),
    (0, "ESU5", 1),
    (0, "NQU5", 1),
    (0, "ESU5", 3),
    (1, "NQU5", 1),
    (1, "ESU5", 1),
    (4, "ESU5", 1),
    (9, "NQU5", 3),
];

const MS: u64 = 1_000_000;

#[test]
fn sample_app_session_throttle_is_deterministic() {
    let session = RateLimiter::new(1000.0, 5);
    let mut sent = 0u64;
    let mut units = 0u64;
    let mut throttled = 0u64;
    for (at_ms, _, weight) in TAPE {
        match session.try_acquire_n_with_retry_at(at_ms * MS, *weight) {
            Acquire::Ok => {
                sent += 1;
                units += weight;
            }
            // The weight-3 cancel-replace at t=0 lands 1ms past the window.
            Acquire::Retry(wait) => {
                throttled += 1;
                assert_eq!(wait, Duration::from_nanos(1_000_000));
            }
            other => panic!("no tape line exceeds the burst, got {other:?}"),
        }
    }
    assert_eq!((sent, units, throttled), (7, 9, 1));

    assert_eq!(
        session.time_until_ready_at(9 * MS, 3),
        Some(Duration::from_nanos(1_000_000))
    );
    assert_eq!(session.time_until_ready_at(9 * MS, 6), None);

    session.reset();
    assert_eq!(
        session.time_until_ready_at(9 * MS, 5),
        Some(Duration::ZERO),
        "a reconnect gets the whole burst back"
    );
}

#[cfg(feature = "keyed")]
#[test]
fn sample_app_per_symbol_quota_is_deterministic() {
    use crate::KeyedRateLimiter;

    let per_symbol = KeyedRateLimiter::new(1000.0, 2);
    let mut sent = 0u64;
    for (at_ms, symbol, _) in TAPE {
        if matches!(
            per_symbol.try_acquire_at(at_ms * MS, symbol, 1),
            Acquire::Ok
        ) {
            sent += 1;
        }
    }
    assert_eq!(sent, 7);
    assert_eq!(per_symbol.len(), 2);
    assert_eq!(per_symbol.retain_active_at(20 * MS), 2);
    assert!(per_symbol.is_empty());
}

#[cfg(feature = "token-bucket")]
#[test]
fn token_bucket_weighted_batch_is_atomic() {
    use std::sync::Arc;

    use crate::{TestClock, TokenBucket};

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

    use crate::{HierarchicalLimiter, TestClock};

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

    use crate::{DistributedLimiter, InMemoryBackend, TestClock};

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

    use crate::{MeteredTokenBucket, TestClock};

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
struct SharedClock(std::sync::Arc<crate::TestClock>);

#[cfg(any(
    feature = "token-bucket",
    feature = "hierarchical",
    feature = "distributed-backend",
    feature = "metrics",
))]
impl crate::Clock for SharedClock {
    fn now_ns(&self) -> u64 {
        self.0.now_ns()
    }
}

#[cfg(feature = "distributed-backend")]
struct SharedBackend(std::sync::Arc<crate::InMemoryBackend>);

#[cfg(feature = "distributed-backend")]
impl crate::Backend for SharedBackend {
    fn incr(&self, key: &str, window_start_ns: u64, ttl_ns: u64) -> u64 {
        self.0.incr(key, window_start_ns, ttl_ns)
    }
    fn read(&self, key: &str, window_start_ns: u64) -> u64 {
        self.0.read(key, window_start_ns)
    }
}
