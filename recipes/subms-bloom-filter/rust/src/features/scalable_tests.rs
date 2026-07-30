use super::*;

#[test]
fn adds_layer_when_saturated() {
    let mut sb = ScalableBloomFilter::new(10);
    for i in 0..30 {
        sb.add(&format!("k{i}"));
    }
    // 10 -> 20 -> 40 capacity progression; 30 entries needs 2-3 layers.
    assert!(
        sb.layer_count() >= 2,
        "expected layer growth, got {}",
        sb.layer_count()
    );
}

#[test]
fn earlier_layer_keys_still_findable() {
    let mut sb = ScalableBloomFilter::new(5);
    for i in 0..50 {
        sb.add(&format!("k{i}"));
    }
    // First-layer keys must still be findable after several layer adds.
    for i in 0..5 {
        assert!(sb.might_contain(&format!("k{i}")), "lost k{i}");
    }
}

#[test]
fn membership_invariant_no_false_negatives() {
    let mut sb = ScalableBloomFilter::new(100);
    for i in 0..500 {
        sb.add(&format!("k{i}"));
    }
    for i in 0..500 {
        assert!(sb.might_contain(&format!("k{i}")));
    }
}

#[test]
fn total_count_tracks_inserts() {
    let mut sb = ScalableBloomFilter::new(10);
    for i in 0..25 {
        sb.add(&format!("k{i}"));
    }
    assert_eq!(sb.total_count(), 25);
}

#[test]
fn empty_scalable_rejects_anything() {
    let sb = ScalableBloomFilter::new(100);
    assert!(!sb.might_contain("never-added"));
    assert_eq!(sb.layer_count(), 1);
    assert_eq!(sb.total_count(), 0);
}

#[test]
fn growth_factor_default_is_two() {
    let mut sb = ScalableBloomFilter::new(4);
    for i in 0..5 {
        sb.add(&format!("k{i}"));
    }
    // First-layer cap 4 -> after 5 adds we should have a second layer of cap 8.
    // Indirect check via total_count + layer_count.
    assert_eq!(sb.layer_count(), 2);
}

#[test]
fn with_growth_uses_custom_factor() {
    let mut sb = ScalableBloomFilter::with_growth(4, 3);
    // Fill layer 0 (cap 4) then force a second layer of cap 12.
    for i in 0..13 {
        sb.add(&format!("k{i}"));
    }
    assert!(sb.layer_count() >= 2);
    assert_eq!(sb.total_count(), 13);
    for i in 0..13 {
        assert!(sb.might_contain(&format!("k{i}")), "lost k{i}");
    }
}

#[test]
fn with_growth_clamps_small_factor_to_two() {
    // growth_factor < 2 would never grow capacity; the constructor clamps to 2.
    let mut sb = ScalableBloomFilter::with_growth(2, 1);
    for i in 0..12 {
        sb.add(&format!("k{i}"));
    }
    // Layers must keep growing (12 entries across cap 2,4,8,... needs >=3 layers).
    assert!(sb.layer_count() >= 3, "got {}", sb.layer_count());
    for i in 0..12 {
        assert!(sb.might_contain(&format!("k{i}")), "lost k{i}");
    }
}

#[test]
fn zero_initial_capacity_clamps_to_one() {
    // initial_capacity.max(1) guards against a zero-sized first layer.
    let mut sb = ScalableBloomFilter::new(0);
    assert_eq!(sb.layer_count(), 1);
    sb.add("a");
    assert!(sb.might_contain("a"));
    // Second add saturates the cap-1 layer, forcing growth.
    sb.add("b");
    assert!(sb.layer_count() >= 2);
    assert!(sb.might_contain("b"));
}

#[test]
fn single_layer_when_under_capacity() {
    let mut sb = ScalableBloomFilter::new(100);
    for i in 0..50 {
        sb.add(&format!("k{i}"));
    }
    assert_eq!(sb.layer_count(), 1, "should not grow before capacity");
    assert_eq!(sb.total_count(), 50);
}
