use super::*;

#[test]
fn add_then_remove_clears_membership() {
    let mut cb = CountingBloomFilter::new(100);
    cb.add("key");
    assert!(cb.might_contain("key"));
    cb.remove("key");
    assert!(!cb.might_contain("key"), "removed key must not match");
}

#[test]
fn remove_of_unknown_is_noop() {
    let mut cb = CountingBloomFilter::new(100);
    cb.add("known");
    cb.remove("unknown-key");
    assert!(
        cb.might_contain("known"),
        "remove of unknown must not poison known"
    );
}

#[test]
fn double_add_survives_single_remove() {
    let mut cb = CountingBloomFilter::new(100);
    cb.add("key");
    cb.add("key");
    cb.remove("key");
    assert!(
        cb.might_contain("key"),
        "after add+add+remove the key still present"
    );
}

#[test]
fn empty_filter_rejects_everything() {
    let cb = CountingBloomFilter::new(100);
    assert!(!cb.might_contain("any"));
}

#[test]
fn saturated_counter_protects_against_overzealous_remove() {
    // Bump the same cell to saturation by adding many distinct keys.
    let mut cb = CountingBloomFilter::new(8); // small filter -> heavy collisions
    for i in 0..1000 {
        cb.add(&format!("k{i}"));
    }
    // One remove should not poison membership of every previously-added key.
    cb.remove("k0");
    let still_present_count = (0..1000)
        .filter(|i| cb.might_contain(&format!("k{i}")))
        .count();
    assert!(still_present_count >= 950, "{}", still_present_count);
}

#[test]
fn fpr_target_holds_with_default_sizing() {
    let mut cb = CountingBloomFilter::new(10_000);
    for i in 0..10_000 {
        cb.add(&format!("present{i}"));
    }
    let probes = 100_000;
    let mut fp = 0;
    for i in 0..probes {
        if cb.might_contain(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    assert!(fpr < 0.05, "FPR {fpr:.4} exceeded 5%");
}

#[test]
fn new_applies_minimum_bit_floor() {
    // Zero expected entries still yields the 64-bit floor, k=7.
    let cb = CountingBloomFilter::new(0);
    assert_eq!(cb.bit_count(), 64);
    assert_eq!(cb.k(), 7);
}

#[test]
fn bit_count_scales_ten_bits_per_key() {
    let cb = CountingBloomFilter::new(1000);
    assert_eq!(cb.bit_count(), 10_000);
    assert_eq!(cb.k(), 7);
}

#[test]
fn remove_on_empty_filter_does_not_underflow() {
    // decr must be a no-op on a zero counter (guards the cur > 0 branch).
    let mut cb = CountingBloomFilter::new(100);
    cb.remove("never-added");
    assert!(!cb.might_contain("never-added"));
    // Filter still usable afterwards.
    cb.add("later");
    assert!(cb.might_contain("later"));
}

#[test]
fn odd_and_even_cells_are_addressed_independently() {
    // Enough distinct keys to exercise both nibbles of shared bytes.
    let mut cb = CountingBloomFilter::new(64);
    for i in 0..200 {
        cb.add(&format!("cell{i}"));
    }
    for i in 0..200 {
        assert!(cb.might_contain(&format!("cell{i}")), "lost cell{i}");
    }
}

#[test]
fn repeated_add_then_matching_removes_clear_key() {
    // Increment a key three times, then three removes should clear it
    // (as long as no other key saturated the shared cells).
    let mut cb = CountingBloomFilter::new(10_000);
    cb.add("x");
    cb.add("x");
    cb.add("x");
    cb.remove("x");
    assert!(cb.might_contain("x"));
    cb.remove("x");
    assert!(cb.might_contain("x"));
    cb.remove("x");
    assert!(!cb.might_contain("x"), "third remove should clear");
}

#[test]
fn no_false_negatives_under_churn() {
    let mut cb = CountingBloomFilter::new(2000);
    for i in 0..2000 {
        cb.add(&format!("k{i}"));
    }
    // Remove the odd half, the even half must remain findable.
    for i in (1..2000).step_by(2) {
        cb.remove(&format!("k{i}"));
    }
    for i in (0..2000).step_by(2) {
        assert!(cb.might_contain(&format!("k{i}")), "lost even k{i}");
    }
}
