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
fn merge_shared_key_takes_max_not_sum() {
    // a saw the key 100 times. b saw it 300 times. Pointwise max
    // gives ~300, pointwise sum would give ~400 - we want the former
    // because both sketches independently absorbed over-estimation.
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
    // Max under the conservative-update bound stays near 300, not 400.
    assert!(est >= 300, "expected >= 300, got {est}");
    assert!(est < 350, "expected close to 300, got {est}");
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
fn merge_is_idempotent_when_src_already_dominated() {
    let mut a = CountMinSketch::new(5, 4096);
    let b = CountMinSketch::new(5, 4096);
    for _ in 0..50 {
        a.add("k");
    }
    // b is empty - all-zero cells - so max(a, b) == a.
    merge_into(&mut a, &b).unwrap();
    let once = a.estimate("k");
    merge_into(&mut a, &b).unwrap();
    let twice = a.estimate("k");
    assert_eq!(once, twice);
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
