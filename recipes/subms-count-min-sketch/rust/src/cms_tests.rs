use super::*;

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn width_rounded_up_to_power_of_two() {
    let cms = CountMinSketch::new(5, 1000);
    assert_eq!(cms.width(), 1024);
}

#[test]
fn estimate_at_or_above_true_count() {
    let mut cms = CountMinSketch::new(5, 16384);
    for _ in 0..1_000 {
        cms.add("hot");
    }
    for _ in 0..10 {
        cms.add("warm");
    }
    assert!(cms.estimate("hot") >= 1_000);
    assert!(cms.estimate("warm") >= 10);
    assert_eq!(cms.estimate("absent"), 0);
}

#[test]
fn over_estimation_is_bounded() {
    // d=5, w=4096; with 1000 distinct keys hashed into 4096 cells, expected
    // over-estimation per cell ~ total/w = 1000/4096 = ~0.24, so a hot key
    // counted once should still report a small number.
    let mut cms = CountMinSketch::new(5, 4096);
    for i in 0..1_000 {
        cms.add(&format!("k{i}"));
    }
    cms.add("HOT");
    let est = cms.estimate("HOT");
    assert!(est >= 1);
    // Conservative update + d=5 means the slop should be modest.
    assert!(est < 10, "got est={est}; over-estimation too high");
}

#[test]
fn depth_floor_is_two() {
    let cms = CountMinSketch::new(0, 1024);
    assert!(cms.depth() >= 2);
}

#[test]
fn unseen_key_returns_zero() {
    let cms = CountMinSketch::new(5, 16384);
    assert_eq!(cms.estimate("never-added"), 0);
}

#[test]
fn single_increment_returns_one_or_more() {
    let mut cms = CountMinSketch::new(5, 16384);
    cms.add("only");
    assert!(cms.estimate("only") >= 1);
}

#[test]
fn estimates_grow_monotonically() {
    let mut cms = CountMinSketch::new(5, 16384);
    let mut prev = 0u32;
    for _ in 0..1000 {
        cms.add("rising");
        let cur = cms.estimate("rising");
        assert!(cur >= prev);
        prev = cur;
    }
}

#[test]
fn many_distinct_keys_dont_explode_unrelated_count() {
    let mut cms = CountMinSketch::new(5, 16384);
    cms.add("focus");
    for i in 0..1_000 {
        cms.add(&format!("noise-{i}"));
    }
    let est = cms.estimate("focus");
    assert!(est >= 1, "focus count >= 1, got {est}");
    assert!(est < 100, "over-estimation way too high: {est}");
}

#[test]
fn d_and_w_accessors_return_chosen_values() {
    let cms = CountMinSketch::new(7, 8192);
    assert_eq!(cms.depth(), 7);
    assert_eq!(cms.width(), 8192);
}

#[test]
fn depth_clamped_to_max_depth() {
    // Rows past MAX_DEPTH would never be read by add or estimate, so the
    // constructor clamps rather than allocating a matrix it will not use.
    let cms = CountMinSketch::new(100, 1024);
    assert_eq!(cms.depth(), MAX_DEPTH);
    assert_eq!(cms.heap_bytes(), MAX_DEPTH * 1024 * 4);
}

#[test]
fn weighted_add_matches_repeated_add() {
    let mut bulk = CountMinSketch::new(5, 4096);
    let mut one_at_a_time = CountMinSketch::new(5, 4096);
    bulk.add_n("ESZ5", 250);
    for _ in 0..250 {
        one_at_a_time.add("ESZ5");
    }
    assert_eq!(bulk.estimate("ESZ5"), one_at_a_time.estimate("ESZ5"));
    assert_eq!(bulk.total(), one_at_a_time.total());
}

#[test]
fn zero_weight_add_is_a_noop() {
    let mut cms = CountMinSketch::new(5, 4096);
    cms.add_n("k", 0);
    assert_eq!(cms.estimate("k"), 0);
    assert_eq!(cms.total(), 0);
}

#[test]
fn total_counts_weight_not_keys() {
    let mut cms = CountMinSketch::new(5, 4096);
    cms.add("a");
    cms.add("b");
    cms.add_n("c", 98);
    assert_eq!(cms.total(), 100);
    assert!(!cms.is_empty());
}

#[test]
fn clear_resets_counters_and_volume() {
    let mut cms = CountMinSketch::new(5, 4096);
    for i in 0..500 {
        cms.add(&format!("k{i}"));
    }
    assert!(cms.occupancy() > 0.0);
    cms.clear();
    assert_eq!(cms.total(), 0);
    assert!(cms.is_empty());
    assert_eq!(cms.estimate("k1"), 0);
    assert_eq!(cms.occupancy(), 0.0);
    // Shape and seed survive so the matrix is reused, not reallocated.
    assert_eq!(cms.depth(), 5);
    assert_eq!(cms.width(), 4096);
}

#[test]
fn byte_and_string_keys_hash_identically() {
    let mut a = CountMinSketch::new(5, 4096);
    let mut b = CountMinSketch::new(5, 4096);
    a.add("ESZ5");
    b.add_bytes(b"ESZ5");
    assert_eq!(a.estimate("ESZ5"), b.estimate_bytes(b"ESZ5"));
    assert_eq!(a.estimate_bytes(b"ESZ5"), 1);
}

#[test]
fn u64_keys_match_their_little_endian_bytes() {
    let mut a = CountMinSketch::new(5, 4096);
    let mut b = CountMinSketch::new(5, 4096);
    a.add_u64(987_654_321);
    b.add_bytes(&987_654_321u64.to_le_bytes());
    assert_eq!(a.estimate_u64(987_654_321), 1);
    assert_eq!(a.estimate_u64(987_654_321), b.estimate_u64(987_654_321));
    a.add_u64_n(42, 7);
    assert_eq!(a.estimate_u64(42), 7);
}

#[test]
fn seed_changes_the_hash_family() {
    // Same key, different seeds: the cells differ, so a sketch full of noise
    // gives different slop. The seed is what stops a caller who can see the
    // keys from steering collisions onto a target.
    let mut a = CountMinSketch::with_seed(4, 64, 0);
    let mut b = CountMinSketch::with_seed(4, 64, 0xdeadbeef);
    for i in 0..500 {
        a.add(&format!("n{i}"));
        b.add(&format!("n{i}"));
    }
    assert_eq!(a.seed(), 0);
    assert_eq!(b.seed(), 0xdeadbeef);
    let differs = (0..200).any(|i| {
        let k = format!("probe{i}");
        a.estimate(&k) != b.estimate(&k)
    });
    assert!(differs, "a reseeded sketch must not collide identically");
}

#[test]
fn default_constructor_is_the_zero_seed() {
    let mut a = CountMinSketch::new(5, 1024);
    let mut b = CountMinSketch::with_seed(5, 1024, 0);
    a.add("k");
    b.add("k");
    assert_eq!(a.to_bytes(), b.to_bytes());
}

#[test]
fn error_bounds_report_the_sizing() {
    let cms = CountMinSketch::new(5, 16384);
    // eps = e / w
    let expected_eps = std::f64::consts::E / 16384.0;
    assert!((cms.relative_error() - expected_eps).abs() < 1e-12);
    // confidence = 1 - e^-d
    assert!((cms.confidence() - (1.0 - (-5.0f64).exp())).abs() < 1e-12);
    assert!(cms.confidence() > 0.99);
}

#[test]
fn suggested_sizing_meets_the_requested_budget() {
    let w = CountMinSketch::suggest_width(0.001);
    assert!(w.is_power_of_two());
    assert!(std::f64::consts::E / w as f64 <= 0.001);

    let d = CountMinSketch::suggest_depth(0.999);
    assert!(1.0 - (-(d as f64)).exp() >= 0.999);
    assert!(d <= MAX_DEPTH);

    let cms = CountMinSketch::with_error_bounds(0.001, 0.999);
    assert!(cms.relative_error() <= 0.001);
    assert!(cms.confidence() >= 0.999);
}

#[test]
fn suggested_sizing_clamps_degenerate_input() {
    assert_eq!(CountMinSketch::suggest_depth(0.0), 2);
    assert_eq!(CountMinSketch::suggest_depth(-1.0), 2);
    assert_eq!(CountMinSketch::suggest_depth(1.0), MAX_DEPTH);
    assert_eq!(CountMinSketch::suggest_depth(1.0 - 1e-12), MAX_DEPTH);
    assert_eq!(CountMinSketch::suggest_width(0.0), 1 << 30);
    assert_eq!(CountMinSketch::suggest_width(-0.5), 1 << 30);
    assert_eq!(CountMinSketch::suggest_width(1e-12), 1 << 30);
    let seeded = CountMinSketch::with_error_bounds_seeded(0.01, 0.99, 11);
    assert_eq!(seeded.seed(), 11);
}

#[test]
fn true_count_sits_inside_the_reported_interval() {
    let mut cms = CountMinSketch::new(5, 4096);
    for i in 0..20_000u64 {
        cms.add_u64(i % 4000);
    }
    // Each of the 4000 keys was added exactly 5 times.
    let truth = 5u32;
    for k in [0u64, 137, 3999] {
        let est = cms.estimate_u64(k);
        let lo = est.saturating_sub(cms.error_margin());
        assert!(est >= truth, "estimate never below truth");
        assert!(lo <= truth, "lower bound never above truth");
    }
    assert_eq!(
        cms.error_margin(),
        (cms.relative_error() * 20_000.0).ceil() as u32
    );
    let mut named = CountMinSketch::new(5, 4096);
    for _ in 0..100 {
        named.add("ESZ5");
    }
    assert!(named.estimate_lower_bound("ESZ5") <= 100);
    assert!(named.estimate("ESZ5") >= 100);
}

#[test]
fn empty_sketch_has_no_error_margin() {
    let cms = CountMinSketch::new(5, 4096);
    assert_eq!(cms.error_margin(), 0);
    assert_eq!(cms.estimate_lower_bound("k"), 0);
    assert_eq!(cms.occupancy(), 0.0);
}

#[test]
fn occupancy_and_footprint_track_the_shape() {
    let mut cms = CountMinSketch::new(4, 1024);
    assert_eq!(cms.heap_bytes(), 4 * 1024 * 4);
    cms.add("one");
    // One key touches one cell per row.
    let expected = 4.0 / (4.0 * 1024.0);
    assert!((cms.occupancy() - expected).abs() < 1e-9);
}

#[test]
fn counter_saturates_instead_of_wrapping() {
    let mut cms = CountMinSketch::new(2, 4);
    cms.add_n("hot", u32::MAX);
    cms.add_n("hot", 1000);
    assert_eq!(cms.estimate("hot"), u32::MAX);
}

#[test]
fn snapshot_round_trips_state_and_shape() {
    let mut cms = CountMinSketch::with_seed(5, 1024, 99);
    for i in 0..2_000 {
        cms.add(&format!("k{i}"));
    }
    cms.add_n("ESZ5", 250);

    let bytes = cms.to_bytes();
    assert_eq!(bytes.len(), 32 + 5 * 1024 * 4);
    let restored = CountMinSketch::from_bytes(&bytes).unwrap();

    assert_eq!(restored.depth(), 5);
    assert_eq!(restored.width(), 1024);
    assert_eq!(restored.seed(), 99);
    assert_eq!(restored.total(), cms.total());
    assert_eq!(restored.estimate("ESZ5"), cms.estimate("ESZ5"));
    assert_eq!(restored.estimate("k7"), cms.estimate("k7"));
    assert_eq!(restored.to_bytes(), bytes);
}

#[test]
fn snapshot_bytes_are_the_pinned_cross_language_form() {
    // The Java port encodes the same sketch to the same bytes. Changing this
    // fixture is a wire-format break, not a test fix.
    let mut cms = CountMinSketch::with_seed(2, 4, 7);
    cms.add("ESZ5");
    cms.add_n("NQZ5", 3);
    let expected = concat!(
        "5355424d53434d53",                 // magic
        "0100",                             // version 1
        "0200",                             // depth 2
        "04000000",                         // width 4
        "0700000000000000",                 // seed 7
        "0400000000000000",                 // total 4
        "03000000010000000000000000000000", // row 0
        "01000000030000000000000000000000", // row 1
    );
    assert_eq!(to_hex(&cms.to_bytes()), expected);
}

#[test]
fn snapshot_rejects_a_foreign_buffer() {
    let mut junk = CountMinSketch::new(2, 4).to_bytes();
    junk[0] = b'X';
    assert_eq!(
        CountMinSketch::from_bytes(&junk).unwrap_err(),
        SnapshotError::BadMagic
    );
    assert_eq!(
        CountMinSketch::from_bytes(b"short").unwrap_err(),
        SnapshotError::Truncated {
            expected: 32,
            actual: 5
        }
    );
}

#[test]
fn snapshot_rejects_a_future_version() {
    let mut bytes = CountMinSketch::new(2, 4).to_bytes();
    bytes[8] = 9;
    assert_eq!(
        CountMinSketch::from_bytes(&bytes).unwrap_err(),
        SnapshotError::UnsupportedVersion(9)
    );
}

#[test]
fn snapshot_rejects_a_bad_shape_or_a_truncated_tail() {
    let mut bytes = CountMinSketch::new(2, 4).to_bytes();
    bytes[10] = 0; // depth 0
    bytes[11] = 0;
    assert_eq!(
        CountMinSketch::from_bytes(&bytes).unwrap_err(),
        SnapshotError::BadShape { depth: 0, width: 4 }
    );

    let mut odd_width = CountMinSketch::new(2, 4).to_bytes();
    odd_width[12] = 5; // width 5 is not a power of two
    assert_eq!(
        CountMinSketch::from_bytes(&odd_width).unwrap_err(),
        SnapshotError::BadShape { depth: 2, width: 5 }
    );

    let full = CountMinSketch::new(2, 4).to_bytes();
    let clipped = &full[..full.len() - 4];
    assert_eq!(
        CountMinSketch::from_bytes(clipped).unwrap_err(),
        SnapshotError::Truncated {
            expected: 32 + 2 * 4 * 4,
            actual: clipped.len()
        }
    );
}

#[test]
fn snapshot_error_messages_name_the_problem() {
    assert_eq!(
        SnapshotError::BadMagic.to_string(),
        "not a count-min-sketch snapshot"
    );
    assert_eq!(
        SnapshotError::UnsupportedVersion(9).to_string(),
        "unsupported snapshot version 9"
    );
    assert_eq!(
        SnapshotError::BadShape { depth: 0, width: 4 }.to_string(),
        "invalid shape: depth=0, width=4"
    );
    assert_eq!(
        SnapshotError::Truncated {
            expected: 32,
            actual: 5
        }
        .to_string(),
        "truncated snapshot: expected 32 bytes, got 5"
    );
    let as_error: &dyn std::error::Error = &SnapshotError::BadMagic;
    assert!(as_error.to_string().contains("snapshot"));
}

#[test]
fn stress_one_sided_error_holds_over_a_skewed_stream() {
    // Zipf-ish: key i appears (1000 / (i+1)) times. The guarantee under test
    // is the only one the structure makes - the estimate is never below truth.
    let mut cms = CountMinSketch::new(5, 8192);
    let mut truth = std::collections::HashMap::new();
    for i in 0..3_000u32 {
        let hits = 1000 / (i + 1) + 1;
        let key = format!("sym-{i}");
        for _ in 0..hits {
            cms.add(&key);
        }
        truth.insert(key, hits);
    }
    let margin = cms.error_margin();
    for (key, &hits) in &truth {
        let est = cms.estimate(key);
        assert!(est >= hits, "under-count on {key}: {est} < {hits}");
        assert!(
            est.saturating_sub(margin) <= hits,
            "lower bound above truth on {key}"
        );
    }
}
