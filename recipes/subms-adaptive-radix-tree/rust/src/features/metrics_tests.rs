use super::*;

#[test]
fn tree_and_tree_mut_expose_the_inner_art() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"a", 1);
    // Read-only borrow of the underlying tree.
    assert_eq!(m.tree().get(b"a").copied(), Some(1));
    // Mutable borrow bypasses the counters (the compaction path).
    let inner = m.tree_mut();
    inner.insert(b"b", 2);
    assert_eq!(m.tree().get(b"b").copied(), Some(2));
}

#[test]
fn empty_metrics_zero_everywhere() {
    let m: MeasuredArt<u32> = MeasuredArt::new();
    let snap = m.metrics();
    assert_eq!(snap.lookups, 0);
    assert_eq!(snap.insertions, 0);
    assert_eq!(snap.deletions, 0);
    assert_eq!(snap.last_depth, 0);
    assert_eq!(snap.entries, 0);
    // Root node always exists - it starts as a Node4 at construction.
    assert_eq!(snap.node_types.node4, 1);
    assert_eq!(snap.node_types.node16, 0);
}

#[test]
fn counters_track_operations() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"alice", 1);
    m.insert(b"bob", 2);
    m.insert(b"alex", 3);
    let _ = m.get(b"alice");
    let _ = m.get(b"missing");
    let _ = m.delete(b"alex");
    let snap = m.metrics();
    assert_eq!(snap.insertions, 3);
    assert_eq!(snap.lookups, 2);
    assert_eq!(snap.deletions, 1);
    assert_eq!(snap.entries, 2);
}

#[test]
fn last_depth_reflects_key_length() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"abcdefghij", 1);
    assert_eq!(m.metrics().last_depth, 10);
    let _ = m.get(b"x");
    assert_eq!(m.metrics().last_depth, 1);
    let _ = m.delete(b"abcdefghij");
    assert_eq!(m.metrics().last_depth, 10);
}

#[test]
fn node_type_distribution_changes_with_growth() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    // Up to 4 distinct first bytes -> root stays a Node4.
    for i in 0..4u8 {
        m.insert(&[i], i as u32);
    }
    // Root Node4 + 4 leaves (each an empty Node4) = 5 Node4, no Node16.
    let before = m.metrics().node_types;
    assert_eq!(before.node16, 0, "no Node16 yet: {before:?}");
    assert_eq!(before.node4, 5, "root + 4 leaves: {before:?}");

    // 5th distinct first byte promotes the root Node4 -> Node16; the 5
    // leaves stay Node4.
    m.insert(&[4u8], 4);
    let after = m.metrics().node_types;
    assert_eq!(after.node16, 1, "root now Node16: {after:?}");
    assert_eq!(after.node4, 5, "the 5 leaves remain Node4: {after:?}");
}

#[test]
fn counter_overflow_uses_saturating_arithmetic() {
    // Force the counters to within striking distance of u64::MAX.
    let m: MeasuredArt<u32> = MeasuredArt::new();
    m.lookups.set(u64::MAX - 1);
    let _ = m.get(b"x");
    assert_eq!(m.metrics().lookups, u64::MAX);
    let _ = m.get(b"y");
    assert_eq!(m.metrics().lookups, u64::MAX, "saturating, not wrapping");
}

#[test]
fn metrics_snapshot_is_independent_of_subsequent_ops() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"a", 1);
    let snap = m.metrics();
    m.insert(b"b", 2);
    m.insert(b"c", 3);
    // The snapshot does not move.
    assert_eq!(snap.insertions, 1);
    assert_eq!(snap.entries, 1);
    // Re-read picks up the new state.
    assert_eq!(m.metrics().insertions, 3);
}

#[test]
fn len_is_empty_and_default() {
    let m: MeasuredArt<u32> = MeasuredArt::default();
    assert!(m.is_empty());
    assert_eq!(m.len(), 0);
    let mut m = m;
    m.insert(b"a", 1);
    m.insert(b"bb", 2);
    assert!(!m.is_empty());
    assert_eq!(m.len(), 2);
}

#[test]
fn walk_counts_node48_and_node256() {
    // 30 distinct first bytes promote the root to Node48.
    let mut m48: MeasuredArt<u32> = MeasuredArt::new();
    for i in 0..30u8 {
        m48.insert(&[i], i as u32);
    }
    let d48 = m48.metrics().node_types;
    assert_eq!(d48.node48, 1, "root is Node48: {d48:?}");
    assert_eq!(d48.node256, 0);

    // A full first-byte fan-out promotes the root to Node256.
    let mut m256: MeasuredArt<u32> = MeasuredArt::new();
    for i in 0..=255u8 {
        m256.insert(&[i], i as u32);
    }
    let d256 = m256.metrics().node_types;
    assert_eq!(d256.node256, 1, "root is Node256: {d256:?}");
}

#[test]
fn tree_accessor_composes_with_base_api() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"alpha", 1);
    m.insert(b"beta", 2);
    // The wrapped tree is queriable through the inner reference.
    assert_eq!(m.tree().get(b"alpha").copied(), Some(1));
    assert_eq!(m.tree().get(b"beta").copied(), Some(2));
    // Inner lookups go through `tree()`, not through `get()`, so
    // the lookup counter stays at 0.
    assert_eq!(m.metrics().lookups, 0);
}

#[test]
fn tree_mut_bypasses_the_counters() {
    let mut m: MeasuredArt<u32> = MeasuredArt::new();
    m.insert(b"alpha", 1);
    // Mutating through the raw handle stays outside the metrics view.
    m.tree_mut().insert(b"beta", 2);
    assert_eq!(m.tree().get(b"beta").copied(), Some(2));
    assert_eq!(m.metrics().insertions, 1);
    assert_eq!(m.metrics().entries, 2);
}
