use super::*;

#[test]
fn delete_returns_prior_value() {
    let mut t: Art<u32> = Art::new();
    t.insert(b"alpha", 1);
    t.insert(b"beta", 2);
    assert_eq!(delete(&mut t, b"alpha"), Some(1));
    assert_eq!(delete(&mut t, b"alpha"), None, "second delete is a no-op");
    assert_eq!(t.len(), 1);
    assert_eq!(t.get(b"beta").copied(), Some(2));
}

#[test]
fn compact_shrinks_full_back_to_small() {
    let mut t: Art<u32> = Art::new();
    // Grow the root to Full (5+ distinct first bytes).
    for i in 0..10u8 {
        t.insert(&[i], i as u32);
    }
    // Delete down to 3 occupants.
    for i in 0..7u8 {
        delete(&mut t, &[i]);
    }
    // Pre-compact: root is Full, has 3 children each with values.
    let changes = compact(&mut t);
    assert!(changes >= 1, "expected at least one shape change");

    // Post-compact: surviving keys still resolvable.
    for i in 7..10u8 {
        assert_eq!(t.get(&[i]).copied(), Some(i as u32));
    }
    // The deleted keys no longer return.
    for i in 0..7u8 {
        assert!(t.get(&[i]).is_none());
    }
}

#[test]
fn compact_is_idempotent() {
    let mut t: Art<u32> = Art::new();
    for i in 0..10u8 {
        t.insert(&[i], i as u32);
    }
    for i in 0..7u8 {
        delete(&mut t, &[i]);
    }
    let first = compact(&mut t);
    let second = compact(&mut t);
    assert!(first >= 1);
    assert_eq!(second, 0, "no further compaction; got {second} changes");
    for i in 7..10u8 {
        assert_eq!(t.get(&[i]).copied(), Some(i as u32));
    }
}

#[test]
fn compact_prunes_empty_subtrees() {
    let mut t: Art<u32> = Art::new();
    // Insert a deep path then delete its terminal value. The
    // intermediate nodes have no value and no other children, so
    // compact() should prune them.
    t.insert(b"hello", 1);
    t.insert(b"world", 2);
    assert_eq!(delete(&mut t, b"hello"), Some(1));

    // Before compact the path "h-e-l-l-o" still exists (no values).
    let changes = compact(&mut t);
    assert!(changes > 0, "pruning should report changes");

    // World survived.
    assert_eq!(t.get(b"world").copied(), Some(2));
    // Hello path is gone.
    assert!(t.get(b"hello").is_none());

    // A second compact is a no-op.
    assert_eq!(compact(&mut t), 0);
}

#[test]
fn compact_on_empty_tree_is_noop() {
    let mut t: Art<u32> = Art::new();
    let changes = compact(&mut t);
    assert_eq!(changes, 0);
    assert_eq!(t.len(), 0);
}

#[test]
fn compact_keeps_full_when_occupancy_above_four() {
    let mut t: Art<u32> = Art::new();
    for i in 0..10u8 {
        t.insert(&[i], i as u32);
    }
    // Delete just two, leaving 8 occupants - still Full.
    delete(&mut t, &[0u8]);
    delete(&mut t, &[1u8]);
    compact(&mut t);
    // Surviving keys still queryable.
    for i in 2..10u8 {
        assert_eq!(t.get(&[i]).copied(), Some(i as u32));
    }
}

#[test]
fn delete_of_missing_key_returns_none() {
    let mut t: Art<u32> = Art::new();
    t.insert(b"present", 1);
    assert!(delete(&mut t, b"absent").is_none());
    assert_eq!(t.len(), 1);
    assert!(delete(&mut t, b"nope_no_path").is_none());
}
