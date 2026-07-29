use std::sync::Arc;

use super::*;
use crate::features::clock::TestClock;

struct ArcClock(Arc<TestClock>);
impl Clock for ArcClock {
    fn now_ns(&self) -> u64 {
        self.0.now_ns()
    }
}

fn build(
    parent_cap: u64,
    parent_rate: f64,
    children: usize,
    child_cap: u64,
    child_rate: f64,
) -> (HierarchicalLimiter, Arc<TestClock>) {
    let clk = Arc::new(TestClock::new());
    let c = clk.clone();
    let h = HierarchicalLimiter::with_clock_fn(
        parent_cap,
        parent_rate,
        children,
        child_cap,
        child_rate,
        || Box::new(ArcClock(c.clone())),
    );
    (h, clk)
}

#[test]
fn parent_caps_total_across_children() {
    // Parent allows 5 total; each child can do 10. So even though
    // two children could grant up to 20 between them, the parent
    // caps the sum at 5.
    let (h, _clk) = build(5, 0.0, 2, 10, 0.0);
    let mut granted = 0;
    for _ in 0..10 {
        if h.try_acquire(0, 1) {
            granted += 1;
        }
    }
    for _ in 0..10 {
        if h.try_acquire(1, 1) {
            granted += 1;
        }
    }
    assert_eq!(granted, 5, "parent must cap total at 5");
}

#[test]
fn child_caps_independent_when_parent_has_budget() {
    // Parent generous (1000); child capped at 3.
    let (h, _clk) = build(1000, 0.0, 1, 3, 0.0);
    for _ in 0..3 {
        assert!(h.try_acquire(0, 1));
    }
    assert!(!h.try_acquire(0, 1), "child capacity exhausted");
}

#[test]
fn unknown_child_id_rejects() {
    let (h, _clk) = build(10, 0.0, 1, 5, 0.0);
    assert!(!h.try_acquire(99, 1), "out-of-range child id should reject");
}

#[test]
fn refill_after_parent_exhaustion_unblocks_children() {
    // Parent 5 cap, 50 tokens/sec = 1 per 20 ms.
    let (h, clk) = build(5, 50.0, 2, 10, 0.0);
    // Drain parent via child 0.
    for _ in 0..5 {
        assert!(h.try_acquire(0, 1));
    }
    assert!(!h.try_acquire(1, 1), "parent exhausted");
    // 100 ms -> 5 tokens refilled.
    clk.advance_ms(100);
    let mut got = 0;
    for _ in 0..10 {
        if h.try_acquire(1, 1) {
            got += 1;
        }
    }
    assert_eq!(got, 5, "exactly 5 parent tokens refilled (then exhausted)");
}

#[test]
fn batch_acquire_atomic_at_both_levels() {
    let (h, _clk) = build(10, 0.0, 1, 10, 0.0);
    assert!(h.try_acquire(0, 7));
    assert!(!h.try_acquire(0, 5), "would exceed parent capacity now");
    assert!(h.try_acquire(0, 3), "exactly 3 left should grant");
    assert!(!h.try_acquire(0, 1));
}

#[test]
fn parent_and_child_accessors_expose_underlying_buckets() {
    let (h, _clk) = build(7, 0.0, 2, 3, 0.0);
    assert_eq!(h.parent().capacity(), 7);
    assert_eq!(h.child(0).map(|c| c.capacity()), Some(3));
    assert_eq!(h.child(99).map(|c| c.capacity()), None);
    assert_eq!(h.num_children(), 2);
}
