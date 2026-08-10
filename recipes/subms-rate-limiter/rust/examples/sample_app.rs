//! Sample app: an order-entry gateway that replays a fixed tape of orders
//! against a venue's published rate limits.
//!
//! Everything runs on a VIRTUAL clock the app steps itself, so the printed
//! output is byte-identical on every run. A rate limiter driven by the wall
//! clock prints a different number each time, which makes it useless as a page
//! example and useless as a regression check.
//!
//! Run the base with `cargo run --example sample_app`; add `--features full`
//! (or a subset like `--features keyed`) to light up the optional shapes.
//!
//! * base - the session throttle, its retry-after, and the planning peek
//! * keyed - a per-symbol quota inside one session
//! * token-bucket - a weighted message budget that banks idle credit
//! * hierarchical - a desk gateway capping two strategy sessions
//! * distributed-backend - one per-account quota across two stateless routers
//! * metrics - the throttle as its own metric source

use subms_rate_limiter::{Acquire, RateLimiter};

/// One line of the order tape: when it arrives (ms into the session), the
/// symbol, and what it costs the venue in message units.
struct Order {
    at_ms: u64,
    symbol: &'static str,
    action: &'static str,
    weight: u64,
}

/// A minute of a quiet morning: a burst of new orders on the open, a heavy
/// cancel-replace, then a trickle.
const TAPE: &[Order] = &[
    Order {
        at_ms: 0,
        symbol: "ESU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 0,
        symbol: "ESU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 0,
        symbol: "NQU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 0,
        symbol: "ESU5",
        action: "cancel-replace",
        weight: 3,
    },
    Order {
        at_ms: 1,
        symbol: "NQU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 1,
        symbol: "ESU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 4,
        symbol: "ESU5",
        action: "new",
        weight: 1,
    },
    Order {
        at_ms: 9,
        symbol: "NQU5",
        action: "cancel-replace",
        weight: 3,
    },
];

const MS: u64 = 1_000_000;

fn main() {
    session_throttle();

    #[cfg(feature = "keyed")]
    per_symbol_quota();

    #[cfg(feature = "token-bucket")]
    weighted_message_budget();

    #[cfg(feature = "hierarchical")]
    desk_gateway_cap();

    #[cfg(feature = "distributed-backend")]
    per_account_quota();

    #[cfg(feature = "metrics")]
    metered_feed_throttle();
}

/// The venue caps this session at 1000 messages/sec with a burst of 5. Each
/// tape line is weighted, so a cancel-replace draws three permits and can be
/// refused whole. A refusal comes back with the wait to put in the venue's
/// throttle response, and the gateway peeks before it commits so it can log
/// the queue depth it is looking at.
fn session_throttle() {
    println!("== session throttle: 1000 msg/sec, burst 5 ==");
    let session = RateLimiter::new(1000.0, 5);

    let mut sent = 0u64;
    let mut units = 0u64;
    for o in TAPE {
        let now = o.at_ms * MS;
        match session.try_acquire_n_with_retry_at(now, o.weight) {
            Acquire::Ok => {
                sent += 1;
                units += o.weight;
                println!("  t={:>2}ms {:<5} {:<14} sent", o.at_ms, o.symbol, o.action);
            }
            Acquire::Retry(wait) => {
                println!(
                    "  t={:>2}ms {:<5} {:<14} throttled, retry after {} us",
                    o.at_ms,
                    o.symbol,
                    o.action,
                    wait.as_micros()
                );
            }
            Acquire::Unattainable { burst_capacity } => {
                println!(
                    "  t={:>2}ms {:<5} {:<14} rejected: weight {} exceeds the burst of {burst_capacity}",
                    o.at_ms, o.symbol, o.action, o.weight
                );
            }
        }
    }
    println!(
        "  -> {sent} of {} messages on the wire, {units} units",
        TAPE.len()
    );
    assert_eq!(sent, 7);
    assert_eq!(units, 9);

    // Planning, not spending: how long before the session could take another
    // cancel-replace at t=9ms, and what a weight nobody can afford looks like.
    let wait = session
        .time_until_ready_at(9 * MS, 3)
        .expect("weight 3 fits a burst of 5");
    println!(
        "  next weight-3 message conforms in {} us",
        wait.as_micros()
    );
    assert_eq!(wait.as_micros(), 1000);
    assert!(
        session.time_until_ready_at(9 * MS, 6).is_none(),
        "weight 6 can never fit a burst of 5"
    );

    // A reconnect gets a fresh allowance from the venue.
    session.reset();
    assert_eq!(
        session.time_until_ready_at(9 * MS, 5),
        Some(std::time::Duration::ZERO)
    );
    println!("  after reconnect: the full burst of 5 is available again");
}

/// `keyed` feature: the venue also caps each SYMBOL, so one hot instrument
/// cannot eat the whole session allowance. State per symbol is the same single
/// TAT, so the whole per-symbol book is one sharded map.
#[cfg(feature = "keyed")]
fn per_symbol_quota() {
    use subms_rate_limiter::KeyedRateLimiter;

    println!("\n== keyed: per-symbol quota, 1000 msg/sec each, burst 2 ==");
    let per_symbol = KeyedRateLimiter::new(1000.0, 2);

    let mut sent = 0u64;
    for o in TAPE {
        let now = o.at_ms * MS;
        if matches!(per_symbol.try_acquire_at(now, o.symbol, 1), Acquire::Ok) {
            sent += 1;
        } else {
            println!(
                "  t={:>2}ms {:<5} throttled on its own quota",
                o.at_ms, o.symbol
            );
        }
    }
    println!("  -> {sent} admitted across {} symbols", per_symbol.len());
    assert_eq!(sent, 7);
    assert_eq!(per_symbol.len(), 2);

    // Housekeeping: a symbol that has gone quiet is back at full burst anyway,
    // so dropping it costs nothing and keeps the map sized to live trading.
    let evicted = per_symbol.retain_active_at(20 * MS);
    println!(
        "  swept at t=20ms: {evicted} idle symbols dropped, {} live",
        per_symbol.len()
    );
    assert_eq!(evicted, 2);
    assert!(per_symbol.is_empty());
}

/// `token-bucket` feature: the same weighted budget, but with a bucket's slack
/// model - credit accumulates while the session is idle, so a quiet minute is
/// followed by a legitimate spike the GCRA window would have smoothed away.
#[cfg(feature = "token-bucket")]
fn weighted_message_budget() {
    use std::sync::Arc;

    use subms_rate_limiter::{TestClock, TokenBucket};

    println!("\n== token-bucket: weighted budget that banks idle credit ==");
    let clock = Arc::new(TestClock::new());
    // 10 units of budget, refilling 5 units/sec.
    let budget = TokenBucket::with_clock(10, 5.0, Box::new(SharedClock(clock.clone())));

    assert!(budget.try_acquire(1), "new order costs 1 unit");
    assert!(budget.try_acquire(5), "bulk cancel-replace costs 5 units");
    println!("  after 1 + 5 units: {} left", budget.available());
    assert_eq!(budget.available(), 4);

    // All-or-nothing: a batch of 5 against 4 remaining spends nothing.
    assert!(
        !budget.try_acquire(5),
        "insufficient budget rejects the batch"
    );
    assert_eq!(budget.available(), 4, "a rejected batch spends nothing");

    clock.advance_ms(1_000); // +5 units, capped at 10
    println!("  after 1s idle: {} left", budget.available());
    assert!(budget.try_acquire(5), "banked credit admits the batch");
}

/// `hierarchical` feature: a desk runs two strategy sessions, each rated for
/// its own flow, but the desk's single venue uplink caps the aggregate below
/// the sum - so one hot strategy cannot starve the other.
#[cfg(feature = "hierarchical")]
fn desk_gateway_cap() {
    use std::sync::Arc;

    use subms_rate_limiter::{HierarchicalLimiter, TestClock};

    println!("\n== hierarchical: desk uplink caps two strategies ==");
    let clock = Arc::new(TestClock::new());
    let c = clock.clone();
    // Uplink admits 5 total; each of the two strategies could do 10 alone.
    let desk =
        HierarchicalLimiter::with_clock_fn(5, 0.0, 2, 10, 0.0, || Box::new(SharedClock(c.clone())));

    let mut sent = 0usize;
    for round in 0..10 {
        if desk.try_acquire(round % 2, 1) {
            sent += 1;
        }
    }
    println!("  strategies offered 10 orders; uplink admitted {sent}");
    assert_eq!(sent, 5, "the parent caps the desk aggregate at 5");
}

/// `distributed-backend` feature: an account's venue quota must hold across a
/// fleet of stateless routers. Both consult the same fixed-window counter (the
/// Redis INCR + EXPIRE shape), so the account cannot beat the cap by spraying
/// orders across routers.
#[cfg(feature = "distributed-backend")]
fn per_account_quota() {
    use std::sync::Arc;

    use subms_rate_limiter::{DistributedLimiter, InMemoryBackend, TestClock};

    println!("\n== distributed-backend: one account quota, two routers ==");
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
    println!("  8 orders sprayed across 2 routers: {admitted} admitted (quota 5)");
    assert_eq!(admitted, 5, "the shared quota holds across both routers");
}

/// `metrics` feature: cap outbound requests to a market-data vendor and let
/// the limiter be its own metric source - grant / reject counts, refill
/// events, live headroom - with no separate observability layer.
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
