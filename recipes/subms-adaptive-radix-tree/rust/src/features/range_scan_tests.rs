use super::*;

fn build(keys: &[&[u8]]) -> Art<u32> {
    let mut t = Art::new();
    for (i, k) in keys.iter().enumerate() {
        t.insert(k, i as u32);
    }
    t
}

#[test]
fn upper_bound_prunes_out_of_range_subtrees() {
    // Subtrees whose first byte already exceeds the upper bound are skipped
    // via the early-continue prune rather than walked.
    let t = build(&[b"a", b"b", b"m", b"z", b"zz"]);
    let got: Vec<Vec<u8>> = range(&t, Bound::Unbounded, Bound::Excluded(b"m"))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn empty_tree_yields_nothing() {
    let t: Art<u32> = Art::new();
    let out = range(&t, Bound::Unbounded, Bound::Unbounded);
    assert!(out.is_empty());
}

#[test]
fn unbounded_scan_returns_all_keys_sorted() {
    let t = build(&[b"banana", b"apple", b"cherry", b"avocado"]);
    let out = range(&t, Bound::Unbounded, Bound::Unbounded);
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        vec![
            b"apple".to_vec(),
            b"avocado".to_vec(),
            b"banana".to_vec(),
            b"cherry".to_vec(),
        ]
    );
}

#[test]
fn inclusive_bounds_both_endpoints_match() {
    let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
    let out = range(&t, Bound::Included(b"b"), Bound::Included(b"d"));
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);
}

#[test]
fn exclusive_bounds_drop_endpoints() {
    let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
    let out = range(&t, Bound::Excluded(b"b"), Bound::Excluded(b"d"));
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"c".to_vec()]);
}

#[test]
fn mixed_bounds() {
    let t = build(&[b"a", b"b", b"c", b"d", b"e"]);
    let out = range(&t, Bound::Included(b"b"), Bound::Excluded(b"d"));
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"b".to_vec(), b"c".to_vec()]);
}

#[test]
fn unbounded_from_returns_prefix() {
    let t = build(&[b"a", b"b", b"c", b"d"]);
    let out = range(&t, Bound::Unbounded, Bound::Included(b"b"));
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn empty_key_is_minimum() {
    let mut t = build(&[b"a", b"b"]);
    t.insert(b"", 99);
    let out = range(&t, Bound::Unbounded, Bound::Unbounded);
    let keys: Vec<Vec<u8>> = out.into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![Vec::<u8>::new(), b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn deep_node_keys_returned_in_order() {
    // Force the root to grow Small -> Full and walks descend deep.
    let mut t = Art::new();
    for i in 0..=255u8 {
        t.insert(&[i, 0, i], i as u32);
    }
    let out = range(&t, Bound::Included(&[10u8]), Bound::Excluded(&[15u8]));
    let starts: Vec<u8> = out.iter().map(|(k, _)| k[0]).collect();
    assert_eq!(starts, vec![10, 11, 12, 13, 14]);
    // Strictly ascending.
    for w in starts.windows(2) {
        assert!(w[0] < w[1]);
    }
}

#[test]
fn out_of_range_bounds_yield_empty() {
    let t = build(&[b"a", b"b", b"c"]);
    let out = range(&t, Bound::Included(b"x"), Bound::Included(b"z"));
    assert!(out.is_empty());
}
