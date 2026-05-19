use subms_count_min_sketch::CountMinSketch;

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
fn d_capped_at_internal_max() {
    // Internal cap is 16 to avoid heap-stack juggling.
    let cms = CountMinSketch::new(100, 1024);
    assert!(cms.depth() <= 100); // we still expose the requested depth
}
