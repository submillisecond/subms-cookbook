use super::*;

#[test]
fn merge_disjoint_keys_preserves_both_counts() {
    let mut a = CountMinSketch::new(5, 4096);
    let mut b = CountMinSketch::new(5, 4096);
    for _ in 0..200 {
        a.add("alpha");
    }
    for _ in 0..150 {
        b.add("beta");
    }
    merge_into(&mut a, &b).unwrap();
    assert!(a.estimate("alpha") >= 200);
    assert!(a.estimate("beta") >= 150);
}

#[test]
fn merge_shared_key_keeps_the_one_sided_guarantee() {
    // The key was seen 100 times on one shard and 300 on the other, so the
    // union count is 400. Summing cells is what keeps the estimate above it;
    // taking the max would report ~300 and break the guarantee.
    let mut a = CountMinSketch::new(5, 16384);
    let mut b = CountMinSketch::new(5, 16384);
    for _ in 0..100 {
        a.add("shared");
    }
    for _ in 0..300 {
        b.add("shared");
    }
    merge_into(&mut a, &b).unwrap();
    let est = a.estimate("shared");
    assert!(est >= 400, "expected >= 400, got {est}");
    assert!(est < 450, "expected close to 400, got {est}");
    assert_eq!(a.total(), 400);
}

#[test]
fn disjoint_merge_takes_max_and_under_counts_overlap() {
    // The documented precondition of merge_disjoint_into is that the shards
    // partition the key space. This pins what happens when it is violated,
    // so the trade-off is a tested fact rather than a caveat in a doc comment.
    let mut a = CountMinSketch::new(5, 16384);
    let mut b = CountMinSketch::new(5, 16384);
    for _ in 0..100 {
        a.add("shared");
    }
    for _ in 0..300 {
        b.add("shared");
    }
    merge_disjoint_into(&mut a, &b).unwrap();
    let est = a.estimate("shared");
    assert!((300..400).contains(&est), "max of the two shards: {est}");
}

#[test]
fn disjoint_merge_is_exact_when_the_precondition_holds() {
    let mut a = CountMinSketch::new(5, 16384);
    let mut b = CountMinSketch::new(5, 16384);
    for _ in 0..100 {
        a.add("us-equities");
    }
    for _ in 0..300 {
        b.add("eu-equities");
    }
    merge_disjoint_into(&mut a, &b).unwrap();
    assert_eq!(a.estimate("us-equities"), 100);
    assert_eq!(a.estimate("eu-equities"), 300);
}

#[test]
fn merge_with_empty_is_noop() {
    let mut a = CountMinSketch::new(5, 4096);
    let empty = CountMinSketch::new(5, 4096);
    for _ in 0..50 {
        a.add("x");
    }
    let before = a.estimate("x");
    merge_into(&mut a, &empty).unwrap();
    assert_eq!(a.estimate("x"), before);
}

#[test]
fn depth_mismatch_errors() {
    let mut a = CountMinSketch::new(5, 4096);
    let b = CountMinSketch::new(7, 4096);
    let err = merge_into(&mut a, &b).unwrap_err();
    match err {
        MergeError::DepthMismatch { dst, src } => {
            assert_eq!(dst, 5);
            assert_eq!(src, 7);
        }
        other => panic!("expected depth mismatch, got {other:?}"),
    }
}

#[test]
fn width_mismatch_errors() {
    let mut a = CountMinSketch::new(5, 4096);
    let b = CountMinSketch::new(5, 8192);
    let err = merge_into(&mut a, &b).unwrap_err();
    match err {
        MergeError::WidthMismatch { dst, src } => {
            assert_eq!(dst, 4096);
            assert_eq!(src, 8192);
        }
        other => panic!("expected width mismatch, got {other:?}"),
    }
}

#[test]
fn seed_mismatch_errors() {
    // Different seeds mean different cells for the same key, so folding the
    // matrices would produce numbers that describe nothing.
    let mut a = CountMinSketch::with_seed(5, 4096, 1);
    let b = CountMinSketch::with_seed(5, 4096, 2);
    let err = merge_into(&mut a, &b).unwrap_err();
    assert_eq!(err, MergeError::SeedMismatch { dst: 1, src: 2 });
    assert_eq!(err.to_string(), "seed mismatch: dst=1, src=2");
}

#[test]
fn merge_error_display_messages() {
    let mut depth = CountMinSketch::new(5, 4096);
    let depth_err = merge_into(&mut depth, &CountMinSketch::new(7, 4096)).unwrap_err();
    assert_eq!(depth_err.to_string(), "depth mismatch: dst=5, src=7");

    let mut width = CountMinSketch::new(5, 4096);
    let width_err = merge_into(&mut width, &CountMinSketch::new(5, 8192)).unwrap_err();
    assert_eq!(width_err.to_string(), "width mismatch: dst=4096, src=8192");

    let as_error: &dyn std::error::Error = &depth_err;
    assert!(as_error.to_string().contains("depth mismatch"));
}

#[test]
fn disjoint_merge_of_empty_src_is_idempotent() {
    let mut a = CountMinSketch::new(5, 4096);
    let b = CountMinSketch::new(5, 4096);
    for _ in 0..50 {
        a.add("k");
    }
    merge_disjoint_into(&mut a, &b).unwrap();
    let once = a.estimate("k");
    merge_disjoint_into(&mut a, &b).unwrap();
    assert_eq!(a.estimate("k"), once);
}

#[test]
fn merged_keys_only_in_src_become_visible() {
    let mut a = CountMinSketch::new(5, 4096);
    let mut b = CountMinSketch::new(5, 4096);
    for _ in 0..75 {
        b.add("only-in-b");
    }
    assert_eq!(a.estimate("only-in-b"), 0);
    merge_into(&mut a, &b).unwrap();
    assert!(a.estimate("only-in-b") >= 75);
}

#[test]
fn fan_in_of_many_shards_bounds_the_union() {
    let mut sink = CountMinSketch::new(5, 8192);
    let mut shards = Vec::new();
    for s in 0..8u32 {
        let mut cms = CountMinSketch::new(5, 8192);
        for _ in 0..(10 * (s + 1)) {
            cms.add("ESZ5");
        }
        shards.push(cms);
    }
    for shard in &shards {
        merge_into(&mut sink, shard).unwrap();
    }
    // 10 + 20 + ... + 80 = 360.
    assert!(sink.estimate("ESZ5") >= 360);
    assert_eq!(sink.total(), 360);
}
