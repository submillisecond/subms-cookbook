use std::sync::Arc;

use super::*;
use crate::features::clock::TestClock;

struct ArcClock(Arc<TestClock>);
impl Clock for ArcClock {
    fn now_ns(&self) -> u64 {
        self.0.now_ns()
    }
}

fn make(cap: u64, rate: f64) -> (MeteredTokenBucket, Arc<TestClock>) {
    let clk = Arc::new(TestClock::new());
    let c = clk.clone();
    let m = MeteredTokenBucket::with_clock(cap, rate, Box::new(ArcClock(c)));
    (m, clk)
}

#[test]
fn counts_granted_and_rejected_distinctly() {
    let (m, _clk) = make(3, 0.0);
    assert!(m.try_acquire(1));
    assert!(m.try_acquire(1));
    assert!(m.try_acquire(1));
    assert!(!m.try_acquire(1));
    assert!(!m.try_acquire(1));
    let s = m.snapshot();
    assert_eq!(s.granted, 3);
    assert_eq!(s.rejected, 2);
}

#[test]
fn snapshot_reflects_current_tokens() {
    let (m, _clk) = make(5, 0.0);
    let s0 = m.snapshot();
    assert_eq!(s0.available, 5);
    m.try_acquire(2);
    let s1 = m.snapshot();
    assert_eq!(s1.available, 3);
}

#[test]
fn refill_events_counted_when_clock_advances() {
    let (m, clk) = make(5, 100.0); // 100/sec -> 1 per 10 ms
    // Drain.
    for _ in 0..5 {
        m.try_acquire(1);
    }
    let s0 = m.snapshot();
    assert_eq!(s0.refills, 0, "no refill yet");
    // 50 ms -> 5 tokens. Next try should see a refill step.
    clk.advance_ms(50);
    assert!(m.try_acquire(1));
    let s1 = m.snapshot();
    assert!(s1.refills >= 1, "refill must be counted at least once");
}

#[test]
fn burst_at_full_does_not_count_refills() {
    let (m, _clk) = make(5, 1000.0);
    for _ in 0..5 {
        m.try_acquire(1);
    }
    // Time hasn't advanced; bucket can't have refilled.
    let s = m.snapshot();
    assert_eq!(s.refills, 0);
}

#[test]
fn try_acquire_one_increments_granted_by_one() {
    let (m, _clk) = make(2, 0.0);
    assert!(m.try_acquire_one());
    let s = m.snapshot();
    assert_eq!(s.granted, 1);
}

#[test]
fn capacity_and_rate_pass_through() {
    let (m, _clk) = make(13, 7.5);
    assert_eq!(m.capacity(), 13);
    assert!((m.rate_per_sec() - 7.5).abs() < 0.01);
}

#[test]
fn snapshot_equality_works_for_assertions() {
    // Just exercises that MetricsSnapshot derives PartialEq.
    let s = MetricsSnapshot {
        granted: 1,
        rejected: 0,
        refills: 0,
        available: 0,
    };
    assert_eq!(
        s,
        MetricsSnapshot {
            granted: 1,
            rejected: 0,
            refills: 0,
            available: 0
        }
    );
}
