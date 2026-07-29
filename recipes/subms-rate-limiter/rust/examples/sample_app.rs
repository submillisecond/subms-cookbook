//! Sample app: a tour of `subms-rate-limiter` in an exchange order-entry
//! setting. Base API first (the lock-free GCRA limiter), then each optional
//! feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features token-bucket`) to light up
//! the feature sections.
//!
//! * base - throttle an order session to the venue's rate, with retry-after
//! * token-bucket - a weighted message budget (order actions cost different weights)
//! * hierarchical - a desk-wide gateway cap shared across strategy sessions
//! * distributed-backend - one per-account quota shared across order routers
//! * metrics - scrape the throttle as its own metric source

use subms_rate_limiter::{Acquire, RateLimiter};

fn main() {
    base_order_session();

    #[cfg(feature = "token-bucket")]
    weighted_message_budget();

    #[cfg(feature = "hierarchical")]
    desk_gateway_cap();

    #[cfg(feature = "distributed-backend")]
    per_account_quota();

    #[cfg(feature = "metrics")]
    metered_feed_throttle();
}

/// Base API: the venue caps this order-entry session at a sustained rate with a
/// small burst allowance. The GCRA limiter grants the burst, then throttles, and
/// `try_acquire_with_retry` hands back the wait to feed a backpressure signal -
/// an HTTP `Retry-After`, or a pause before the next order goes on the wire.
fn base_order_session() {
    println!("== base: order-entry session throttle ==");
    // 100 orders/sec sustained, burst allowance of 5.
    let session = RateLimiter::new(100.0, 5);

    let mut granted = 0usize;
    for i in 1..=8 {
        if session.try_acquire() {
            println!("  order {i:>2} sent");
            granted += 1;
        } else {
            println!("  order {i:>2} throttled");
        }
    }
    println!("  -> {granted} of 8 orders admitted (burst = 5)");
    assert_eq!(granted, 5, "burst allowance admits exactly 5");

    match session.try_acquire_with_retry() {
        Acquire::Ok => panic!("session is saturated; a grant here would be wrong"),
        Acquire::Retry(wait) => {
            println!("  backpressure: retry after {} us", wait.as_micros());
            assert!(
                !wait.is_zero(),
                "a throttled caller must get a positive wait"
            );
        }
    }
}

/// `token-bucket` feature: the venue meters the session by message-units, not by
/// raw order count - a new order costs 1, a bulk cancel-replace costs 5. The
/// bucket drains a variable batch atomically (an under-budget batch spends
/// nothing) and refills up to capacity, so an idle session can spend a burst.
#[cfg(feature = "token-bucket")]
fn weighted_message_budget() {
    use std::sync::Arc;

    use subms_rate_limiter::{TestClock, TokenBucket};

    println!("\n== token-bucket: weighted message budget ==");
    let clock = Arc::new(TestClock::new());
    // Budget of 10 units, refilling 5 units/sec.
    let budget = TokenBucket::with_clock(10, 5.0, Box::new(SharedClock(clock.clone())));

    assert!(budget.try_acquire(1), "new order costs 1 unit");
    assert!(budget.try_acquire(5), "bulk cancel-replace costs 5 units");
    println!("  after 1 + 5 units: {} left", budget.available());
    assert_eq!(budget.available(), 4);

    // A second bulk action needs 5 but only 4 remain: the whole batch is
    // rejected and the partial 4 are left untouched.
    assert!(
        !budget.try_acquire(5),
        "insufficient budget rejects the whole batch"
    );
    assert_eq!(budget.available(), 4, "a rejected batch spends nothing");

    clock.advance_ms(1_000); // 5 units/sec -> +5, capped at 10
    println!("  after 1s refill: {} left", budget.available());
    assert!(budget.try_acquire(5), "refilled budget admits the batch");
}

/// `hierarchical` feature: a trading desk runs two strategy sessions, each rated
/// for its own order flow, but the desk's single venue gateway caps the
/// aggregate below the sum of the children - so one hot strategy cannot starve
/// the shared uplink.
#[cfg(feature = "hierarchical")]
fn desk_gateway_cap() {
    use std::sync::Arc;

    use subms_rate_limiter::{HierarchicalLimiter, TestClock};

    println!("\n== hierarchical: desk-wide gateway cap ==");
    let clock = Arc::new(TestClock::new());
    let c = clock.clone();
    // Parent gateway admits 5 total; two child strategies could each do 10.
    let desk =
        HierarchicalLimiter::with_clock_fn(5, 0.0, 2, 10, 0.0, || Box::new(SharedClock(c.clone())));

    let mut sent = 0usize;
    for round in 0..10 {
        let strategy = round % 2;
        if desk.try_acquire(strategy, 1) {
            sent += 1;
        }
    }
    println!("  strategies offered 10 orders; gateway admitted {sent}");
    assert_eq!(sent, 5, "the parent caps the desk aggregate at 5");
}

/// `distributed-backend` feature: an account's venue quota must hold across a
/// fleet of stateless order routers. Both routers consult the same fixed-window
/// counter (the Redis-style INCR + EXPIRE shape), so the account cannot beat the
/// cap by spraying orders across routers.
#[cfg(feature = "distributed-backend")]
fn per_account_quota() {
    use std::sync::Arc;

    use subms_rate_limiter::{DistributedLimiter, InMemoryBackend, TestClock};

    println!("\n== distributed-backend: per-account quota across routers ==");
    let clock = Arc::new(TestClock::new());
    let shared = Arc::new(InMemoryBackend::new());
    let window_ns = 1_000_000_000u64; // 1s window, 5 orders per account

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

    let account = "acct-42";
    let mut admitted = 0usize;
    for round in 0..8 {
        let router = if round % 2 == 0 { &router_a } else { &router_b };
        if router.try_acquire(account) {
            admitted += 1;
        }
    }
    println!("  two routers, one account: {admitted} of 8 admitted (quota 5)");
    assert_eq!(admitted, 5, "the shared quota holds across both routers");
}

/// `metrics` feature: cap outbound requests to a market-data vendor and let the
/// limiter be its own metric source - grant / reject counts, refill events, and
/// live headroom scraped through `snapshot()` with no separate observability
/// layer.
#[cfg(feature = "metrics")]
fn metered_feed_throttle() {
    use std::sync::Arc;

    use subms_rate_limiter::{MeteredTokenBucket, TestClock};

    println!("\n== metrics: self-observing market-data throttle ==");
    let clock = Arc::new(TestClock::new());
    // 5 requests per burst, refilling 100/sec.
    let feed = MeteredTokenBucket::with_clock(5, 100.0, Box::new(SharedClock(clock.clone())));

    for _ in 0..8 {
        feed.try_acquire(1);
    }
    let s = feed.snapshot();
    println!(
        "  granted {}, rejected {}, headroom {}",
        s.granted, s.rejected, s.available
    );
    assert_eq!(s.granted, 5);
    assert_eq!(s.rejected, 3);

    clock.advance_ms(100); // 100/sec -> +10, capped at 5
    assert!(feed.try_acquire(1), "the refilled feed admits again");
    let s2 = feed.snapshot();
    println!(
        "  after refill: granted {}, refills {}",
        s2.granted, s2.refills
    );
    assert_eq!(s2.granted, 6);
    assert!(s2.refills >= 1, "a refill step was observed");
}

// The feature limiters take an owned `Box<dyn Clock>`, so a test-driven clock
// has to be shared through this newtype rather than handed in directly.
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

// Forwards to one shared in-memory backend so two limiters (two routers) hit
// the same counter, the way two processes would share a Redis instance.
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
