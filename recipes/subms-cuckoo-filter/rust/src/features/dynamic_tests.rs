use super::*;

#[test]
fn round_trip_below_threshold() {
    let mut d = DynamicCuckooFilter::new(2000);
    for i in 0..500u32 {
        assert!(d.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(d.contains(&format!("k{i}")));
    }
    assert_eq!(d.layer_count(), 1, "no grow expected below threshold");
}

#[test]
fn grows_when_threshold_crossed() {
    // 256 cap -> threshold 0.95 ~ 243 entries triggers a grow.
    let mut d = DynamicCuckooFilter::with_threshold(256, 0.5);
    for i in 0..1000u32 {
        assert!(d.insert(&format!("k{i}")));
    }
    assert!(
        d.layer_count() >= 2,
        "expected growth, got {} layers",
        d.layer_count()
    );
    // Membership invariant: every inserted key is findable across layers.
    for i in 0..1000u32 {
        assert!(d.contains(&format!("k{i}")), "lost k{i}");
    }
}

#[test]
fn delete_walks_layers_newest_first() {
    let mut d = DynamicCuckooFilter::with_threshold(64, 0.25);
    for i in 0..200u32 {
        d.insert(&format!("k{i}"));
    }
    assert!(d.layer_count() >= 2);
    // Every previously-inserted key deletes once.
    for i in 0..200u32 {
        assert!(d.delete(&format!("k{i}")), "could not delete k{i}");
    }
    assert_eq!(d.len(), 0);
}

#[test]
fn delete_unknown_returns_false() {
    let mut d = DynamicCuckooFilter::new(100);
    d.insert("known");
    assert!(!d.delete("never-inserted"));
    assert!(d.contains("known"));
}

#[test]
fn empty_filter_rejects_anything() {
    let d = DynamicCuckooFilter::new(100);
    assert!(!d.contains("any"));
    assert!(d.is_empty());
}

#[test]
fn load_factor_resets_after_grow() {
    let mut d = DynamicCuckooFilter::with_threshold(64, 0.4);
    for i in 0..200u32 {
        d.insert(&format!("k{i}"));
    }
    // After growing, the active (newest) layer is the relevant one
    // for load_factor; it should be far below 1.0.
    assert!(d.load_factor() < 1.0);
}

#[test]
fn cumulative_len_tracks_all_layers() {
    let mut d = DynamicCuckooFilter::with_threshold(64, 0.3);
    let n = 500;
    for i in 0..n {
        d.insert(&format!("k{i}"));
    }
    assert_eq!(d.len(), n as usize);
    assert!(d.layer_count() >= 2);
}

#[test]
fn invalid_threshold_falls_back_to_default() {
    // NaN, 0, 1+ all fall back to 0.95.
    let d = DynamicCuckooFilter::with_threshold(100, f64::NAN);
    assert!((d.grow_threshold - 0.95).abs() < 1e-12);
    let d2 = DynamicCuckooFilter::with_threshold(100, 1.5);
    assert!((d2.grow_threshold - 0.95).abs() < 1e-12);
}
