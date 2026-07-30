use std::sync::Arc;

use super::*;
use crate::features::clock::TestClock;

fn bucket(capacity: u64, rate: f64) -> (TokenBucket, Arc<TestClock>) {
    let clock = Arc::new(TestClock::new());
    let c2: Arc<TestClock> = clock.clone();
    let tb = TokenBucket::with_clock(capacity, rate, Box::new(ArcClock(c2)));
    (tb, clock)
}

// Adapter so an Arc<TestClock> can be handed in as Box<dyn Clock>;
// tests need to keep their own handle to advance time.
struct ArcClock(Arc<TestClock>);
impl Clock for ArcClock {
    fn now_ns(&self) -> u64 {
        self.0.now_ns()
    }
}

#[test]
fn burst_at_full_bucket_drains_capacity() {
    let (tb, _clk) = bucket(10, 100.0);
    // Should grant the first 10 (full bucket), then reject.
    for i in 0..10 {
        assert!(tb.try_acquire(1), "draw {i} should succeed");
    }
    assert!(!tb.try_acquire(1), "11th draw should fail with no refill");
}

#[test]
fn refill_over_time_gap_replenishes_bucket() {
    let (tb, clk) = bucket(10, 100.0); // 100 tokens/sec = 1 per 10 ms
    // Drain.
    for _ in 0..10 {
        assert!(tb.try_acquire(1));
    }
    assert!(!tb.try_acquire(1));
    // Advance 50 ms -> 5 tokens.
    clk.advance_ms(50);
    for _ in 0..5 {
        assert!(tb.try_acquire(1));
    }
    assert!(!tb.try_acquire(1));
}

#[test]
fn refill_caps_at_capacity() {
    let (tb, clk) = bucket(5, 10.0);
    // Drain.
    for _ in 0..5 {
        tb.try_acquire(1);
    }
    // Advance way beyond capacity * 1/rate (capacity refill in 500 ms; advance 10 s).
    clk.advance_ms(10_000);
    assert_eq!(tb.available(), 5, "refill must cap at capacity");
}

#[test]
fn batch_acquire_succeeds_or_fails_atomically() {
    let (tb, _clk) = bucket(10, 0.0); // no refill
    assert!(tb.try_acquire(7));
    // 3 left; ask for 5 -> reject WITHOUT spending the remaining 3.
    assert!(!tb.try_acquire(5));
    assert!(tb.try_acquire(3));
    assert!(!tb.try_acquire(1));
}

#[test]
fn zero_acquire_is_always_true_and_does_not_spend() {
    let (tb, _clk) = bucket(3, 0.0);
    assert!(tb.try_acquire(0));
    assert_eq!(tb.available(), 3);
}

#[test]
fn try_acquire_one_is_shorthand_for_one() {
    let (tb, _clk) = bucket(2, 0.0);
    assert!(tb.try_acquire_one());
    assert!(tb.try_acquire_one());
    assert!(!tb.try_acquire_one());
}

#[test]
fn fractional_refill_accumulates_without_loss() {
    // 1 token per second; advance 100 ms ten times. Should net 1 token.
    let (tb, clk) = bucket(5, 1.0);
    for _ in 0..5 {
        tb.try_acquire(1);
    }
    for _ in 0..10 {
        clk.advance_ms(100);
    }
    assert_eq!(tb.available(), 1, "fractional refills must accumulate");
}

#[test]
fn accessors_reflect_construction() {
    let (tb, _clk) = bucket(8, 250.0);
    assert_eq!(tb.capacity(), 8);
    assert!((tb.rate_per_sec() - 250.0).abs() < 0.01);
}

#[test]
fn new_uses_system_clock_and_starts_full() {
    // Exercises the SystemClock-backed default constructor.
    let tb = TokenBucket::new(4, 100.0);
    assert_eq!(tb.capacity(), 4);
    assert_eq!(tb.available(), 4, "bucket starts full");
    for _ in 0..4 {
        assert!(tb.try_acquire_one());
    }
    assert!(!tb.try_acquire_one(), "drained within a sub-ms window");
}

#[test]
fn capacity_and_rate_floors_are_enforced() {
    // cap floors to 1, negative rate floors to 0 (no refill).
    let tb = TokenBucket::with_clock(0, -5.0, Box::new(TestClock::new()));
    assert_eq!(tb.capacity(), 1);
    assert!((tb.rate_per_sec() - 0.0).abs() < f64::EPSILON);
    assert!(tb.try_acquire_one());
    assert!(!tb.try_acquire_one());
}
