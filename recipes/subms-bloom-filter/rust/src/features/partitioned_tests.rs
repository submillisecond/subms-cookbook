use super::*;

#[test]
fn added_keys_always_match() {
    let mut pb = PartitionedBloomFilter::new(1000);
    for i in 0..1000 {
        pb.add(&format!("k{i}"));
    }
    for i in 0..1000 {
        assert!(pb.might_contain(&format!("k{i}")));
    }
}

#[test]
fn empty_filter_rejects_everything() {
    let pb = PartitionedBloomFilter::new(100);
    assert!(!pb.might_contain("any"));
}

#[test]
fn fpr_target_holds_with_default_sizing() {
    let mut pb = PartitionedBloomFilter::new(10_000);
    for i in 0..10_000 {
        pb.add(&format!("present{i}"));
    }
    let probes = 100_000;
    let mut fp = 0;
    for i in 0..probes {
        if pb.might_contain(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    assert!(fpr < 0.05, "FPR {fpr:.4} exceeded 5%");
}

#[test]
fn slice_count_equals_k() {
    let pb = PartitionedBloomFilter::new(100);
    assert_eq!(pb.slices.len(), pb.k() as usize);
}

#[test]
fn add_to_slice_independent_of_full_add() {
    let mut pb = PartitionedBloomFilter::new(100);
    // Per-slice add for a key WITHOUT a full add: membership
    // should be NO because only one slice has the bit.
    pb.add_to_slice("partial", 0);
    assert!(!pb.might_contain("partial"));

    // Now do the per-slice add for all k slices manually: should
    // become a match.
    for i in 1..pb.k() as usize {
        pb.add_to_slice("partial", i);
    }
    assert!(pb.might_contain("partial"));
}

#[test]
#[should_panic(expected = "slice index out of range")]
fn add_to_slice_rejects_bad_index() {
    let mut pb = PartitionedBloomFilter::new(100);
    pb.add_to_slice("k", 999);
}

#[test]
fn geometry_accessors_are_consistent() {
    let pb = PartitionedBloomFilter::new(1000);
    assert_eq!(pb.k(), 7);
    // bit_count == slice_bits * k, and slice_bits == ceil(10*1000 / 7).
    assert_eq!(pb.bit_count(), pb.slice_bits() * pb.k());
    assert!(pb.slice_bits() >= (10_000u32).div_ceil(7));
}

#[test]
fn minimum_sizing_applies_bit_floor() {
    // Zero expected entries -> 64-bit floor spread across 7 slices.
    let pb = PartitionedBloomFilter::new(0);
    assert_eq!(pb.k(), 7);
    assert_eq!(pb.slice_bits(), (64u32).div_ceil(7));
}

#[test]
fn add_to_slice_boundary_index_allowed() {
    // The highest valid slice index (k-1) must not panic.
    let mut pb = PartitionedBloomFilter::new(100);
    let last = pb.k() as usize - 1;
    pb.add_to_slice("edge", last);
    // Single-slice add is not a full membership.
    assert!(!pb.might_contain("edge"));
}

#[test]
fn full_add_sets_all_slices() {
    // A regular add followed by manual per-slice adds is idempotent for membership.
    let mut pb = PartitionedBloomFilter::new(500);
    pb.add("x");
    assert!(pb.might_contain("x"));
    for i in 0..pb.k() as usize {
        pb.add_to_slice("x", i);
    }
    assert!(pb.might_contain("x"));
}

#[test]
fn no_false_negatives_stress() {
    let mut pb = PartitionedBloomFilter::new(5000);
    for i in 0..5000 {
        pb.add(&format!("item-{i}"));
    }
    for i in 0..5000 {
        assert!(pb.might_contain(&format!("item-{i}")), "lost item-{i}");
    }
}
