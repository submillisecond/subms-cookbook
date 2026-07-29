use super::*;

#[test]
fn add_visible_in_current_and_total() {
    let mut w = WindowedCountMinSketch::new(4, 5, 1024);
    for _ in 0..100 {
        w.add("k");
    }
    assert!(w.estimate_current("k") >= 100);
    assert!(w.estimate("k") >= 100);
}

#[test]
fn oldest_slice_drops_off_after_full_rotation() {
    let mut w = WindowedCountMinSketch::new(3, 5, 1024);
    for _ in 0..50 {
        w.add("k");
    }
    let after_first = w.estimate("k");
    assert!(after_first >= 50);
    // Rotate forward 3 times - the slice with the 50 hits gets cleared.
    w.tick();
    w.tick();
    w.tick();
    // After a full lap, the head is back to the original slice (now zeroed),
    // and the slices in between were untouched (no adds), so total -> 0.
    assert_eq!(w.estimate("k"), 0);
}

#[test]
fn adds_after_tick_land_in_new_slice() {
    let mut w = WindowedCountMinSketch::new(4, 5, 1024);
    for _ in 0..20 {
        w.add("a");
    }
    w.tick();
    for _ in 0..30 {
        w.add("b");
    }
    // Current slice has only "b".
    assert!(
        w.estimate_current("a") <= 1,
        "a should not be in the new slice"
    );
    assert!(w.estimate_current("b") >= 30);
    // Total still sees both because "a" hasn't rotated out yet.
    assert!(w.estimate("a") >= 20);
    assert!(w.estimate("b") >= 30);
}

#[test]
fn empty_window_estimates_zero() {
    let w = WindowedCountMinSketch::new(4, 5, 1024);
    assert_eq!(w.estimate("nothing"), 0);
    assert_eq!(w.estimate_current("nothing"), 0);
}

#[test]
fn slice_count_floor_is_two() {
    let w = WindowedCountMinSketch::new(0, 5, 1024);
    assert!(w.slices() >= 2);
}

#[test]
fn estimate_sums_across_active_slices() {
    let mut w = WindowedCountMinSketch::new(3, 5, 4096);
    for _ in 0..10 {
        w.add("x");
    }
    w.tick();
    for _ in 0..10 {
        w.add("x");
    }
    w.tick();
    for _ in 0..10 {
        w.add("x");
    }
    // All three slices contributed ~10 each. The summed bound is
    // always >= 30 (and quite close in practice given low collisions).
    assert!(w.estimate("x") >= 30);
}
