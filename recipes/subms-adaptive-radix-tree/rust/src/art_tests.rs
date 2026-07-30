use super::*;

#[test]
fn insert_and_get() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"alice", 1);
    t.insert(b"bob", 2);
    t.insert(b"alex", 3);
    assert_eq!(t.get(b"alice").copied(), Some(1));
    assert_eq!(t.get(b"bob").copied(), Some(2));
    assert_eq!(t.get(b"alex").copied(), Some(3));
    assert_eq!(t.get(b"missing"), None);
    assert_eq!(t.len(), 3);
}

#[test]
fn insert_replaces_value() {
    let mut t: Art<&'static str> = Art::new();
    assert!(t.insert(b"k", "first").is_none());
    assert_eq!(t.insert(b"k", "second"), Some("first"));
    assert_eq!(t.get(b"k").copied(), Some("second"));
    assert_eq!(t.len(), 1);
}

#[test]
fn empty_key_supported() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"", 42);
    assert_eq!(t.get(b"").copied(), Some(42));
    assert_eq!(t.len(), 1);
}

#[test]
fn many_keys_force_node_growth() {
    let mut t: Art<i32> = Art::new();
    // 256 distinct first-byte keys at the root forces the root to grow
    // from Small (4 slots) to Full (256-way).
    for i in 0..=255u8 {
        let key = [i, 0u8, 0u8];
        t.insert(&key, i as i32);
    }
    assert_eq!(t.len(), 256);
    for i in 0..=255u8 {
        let key = [i, 0u8, 0u8];
        assert_eq!(t.get(&key).copied(), Some(i as i32), "key starting {i}");
    }
}

#[test]
fn shared_prefixes() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"prefix/a", 1);
    t.insert(b"prefix/b", 2);
    t.insert(b"prefix/c", 3);
    assert_eq!(t.get(b"prefix/a").copied(), Some(1));
    assert_eq!(t.get(b"prefix/b").copied(), Some(2));
    assert_eq!(t.get(b"prefix/c").copied(), Some(3));
    assert_eq!(t.get(b"prefix").copied(), None);
}

#[test]
fn empty_tree_is_empty() {
    let t: Art<i32> = Art::new();
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    assert!(t.get(b"any").is_none());
}

#[test]
fn long_keys() {
    let mut t: Art<u32> = Art::new();
    let key = vec![b'a'; 1000];
    t.insert(&key, 42);
    assert_eq!(t.get(&key).copied(), Some(42));
}

#[test]
fn binary_keys_with_zero_bytes() {
    let mut t: Art<u32> = Art::new();
    t.insert(&[0u8, 1, 2], 1);
    t.insert(&[0u8, 1, 3], 2);
    t.insert(&[0u8, 2, 0], 3);
    assert_eq!(t.get(&[0u8, 1, 2]).copied(), Some(1));
    assert_eq!(t.get(&[0u8, 1, 3]).copied(), Some(2));
    assert_eq!(t.get(&[0u8, 2, 0]).copied(), Some(3));
}

#[test]
fn shorter_key_is_distinct_from_longer_with_same_prefix() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"foo", 1);
    t.insert(b"foobar", 2);
    assert_eq!(t.get(b"foo").copied(), Some(1));
    assert_eq!(t.get(b"foobar").copied(), Some(2));
    assert!(t.get(b"foob").is_none());
    assert!(t.get(b"foobaz").is_none());
}

#[test]
fn default_constructor() {
    let t: Art<i32> = Art::default();
    assert!(t.is_empty());
}

#[test]
fn growth_boundaries_node4_16_48_256() {
    // Cross each adaptive-node transition by fan-out at the root: 5 children
    // promotes Node4->Node16, 17 -> Node48, 49 -> Node256. Every key stays
    // retrievable across the promotions.
    for n in [5usize, 17, 49, 256] {
        let mut t: Art<usize> = Art::new();
        for i in 0..n {
            t.insert(&[i as u8], i);
        }
        assert_eq!(t.len(), n, "n={n}");
        for i in 0..n {
            assert_eq!(t.get(&[i as u8]).copied(), Some(i), "n={n} key={i}");
        }
    }
}

// The base insert/get path reaches only a subset of the adaptive-node arms.
// The feature-only methods (remove / take_all / each_child_mut / sorted_pairs
// / get_mut) and the pub(crate) accessors are driven directly here, at every
// node size, so each Node4 / Node16 / Node48 / Node256 arm is exercised.

fn filled_children(n: usize) -> Children<i32> {
    let mut c = Children::new();
    for b in 0..n {
        c.insert(b as u8, Box::new(Node::leaf(Vec::new(), b as i32)));
    }
    c
}

#[test]
fn children_kind_len_and_lookup_across_sizes() {
    let cases = [
        (3usize, NodeKind::Node4),
        (12, NodeKind::Node16),
        (40, NodeKind::Node48),
        (256, NodeKind::Node256),
    ];
    for (n, kind) in cases {
        let mut c = filled_children(n);
        assert_eq!(c.kind(), kind, "n={n}");
        assert_eq!(c.len(), n, "n={n}");
        assert!(!c.is_empty());
        for b in 0..n {
            assert!(c.get(b as u8).is_some(), "n={n} b={b}");
            assert!(c.get_mut(b as u8).is_some(), "n={n} b={b}");
        }
        if n < 256 {
            assert!(c.get(255).is_none());
            assert!(c.get_mut(255).is_none());
        }
    }
}

#[test]
fn children_sorted_pairs_are_ascending_across_sizes() {
    for n in [3usize, 12, 40, 256] {
        let c = filled_children(n);
        let pairs = c.sorted_pairs();
        assert_eq!(pairs.len(), n, "n={n}");
        for w in pairs.windows(2) {
            assert!(w[0].0 < w[1].0, "n={n} not ascending");
        }
    }
}

#[test]
fn children_each_child_mut_visits_all_across_sizes() {
    for n in [3usize, 12, 40, 256] {
        let mut c = filled_children(n);
        let mut seen = 0usize;
        c.each_child_mut(|_| seen += 1);
        assert_eq!(seen, n, "n={n}");
    }
}

#[test]
fn children_remove_across_sizes() {
    for n in [4usize, 16, 48, 256] {
        let mut c = filled_children(n);
        assert!(c.remove(1).is_some(), "n={n} remove present");
        assert_eq!(c.len(), n - 1, "n={n}");
        assert!(c.get(1).is_none(), "n={n}");
        if n < 256 {
            // A byte that was never inserted drives the None arm.
            assert!(c.remove(200).is_none(), "n={n} remove absent");
        }
    }
}

#[test]
fn children_take_all_resets_to_node4_across_sizes() {
    for n in [3usize, 12, 40, 256] {
        let mut c = filled_children(n);
        let taken = c.take_all();
        assert_eq!(taken.len(), n, "n={n}");
        assert_eq!(c.kind(), NodeKind::Node4, "n={n}");
        assert!(c.is_empty(), "n={n}");
    }
}

#[test]
fn children_get_or_insert_for_load_is_idempotent() {
    let mut c: Children<i32> = Children::new();
    let _ = c.get_or_insert_for_load(7);
    assert!(c.get(7).is_some());
    assert_eq!(c.len(), 1);
    // Present -> returns the existing node, no second insert.
    let _ = c.get_or_insert_for_load(7);
    assert_eq!(c.len(), 1);
}

#[test]
fn art_internal_accessors_and_delete_value() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"key", 1);
    t.insert(b"other", 2);
    let _ = t.root();
    let _ = t.root_mut();
    assert_eq!(t.delete_value(b"key"), Some(1));
    assert_eq!(t.len(), 1);
    // Value already cleared -> None, but the node still exists.
    assert_eq!(t.delete_value(b"key"), None);
    // A key whose path diverges -> walk_mut returns None.
    assert_eq!(t.delete_value(b"absent-branch"), None);
    // Raw len setter used by deserialize.
    t.set_len(9);
    assert_eq!(t.len(), 9);
}

#[test]
fn get_mut_descends_existing_children_at_large_nodes() {
    // Insert 256 first-byte children (root -> Node256), then insert a second
    // key under each existing first byte so insert_rec descends through
    // get_mut rather than creating a fresh child.
    let mut t: Art<i32> = Art::new();
    for b in 0u8..=255 {
        t.insert(&[b, 10], b as i32);
    }
    for b in 0u8..=255 {
        t.insert(&[b, 20], b as i32 + 1000);
    }
    for b in 0u8..=255 {
        assert_eq!(t.get(&[b, 10]).copied(), Some(b as i32));
        assert_eq!(t.get(&[b, 20]).copied(), Some(b as i32 + 1000));
    }
}

#[test]
fn multi_level_path_compression_splits() {
    let mut t: Art<i32> = Art::new();
    // A shared stem that splits at successively deeper points.
    t.insert(b"abcdefg", 1);
    t.insert(b"abcdefh", 2); // splits at the 7th byte
    t.insert(b"abcxyz", 3); //  splits the "abc..." node at the 4th byte
    t.insert(b"abc", 4); //     a key that is a prefix of the others
    t.insert(b"a", 5); //       an even shorter prefix key
    assert_eq!(t.len(), 5);
    assert_eq!(t.get(b"abcdefg").copied(), Some(1));
    assert_eq!(t.get(b"abcdefh").copied(), Some(2));
    assert_eq!(t.get(b"abcxyz").copied(), Some(3));
    assert_eq!(t.get(b"abc").copied(), Some(4));
    assert_eq!(t.get(b"a").copied(), Some(5));
    // Interior points that carry no value must still miss.
    assert_eq!(t.get(b"ab"), None);
    assert_eq!(t.get(b"abcd"), None);
    assert_eq!(t.get(b"abcde"), None);
    assert_eq!(t.get(b"abcdef"), None);
}
