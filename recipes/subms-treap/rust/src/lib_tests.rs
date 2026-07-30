//! Crate-level unit tests. Colocated with `lib.rs` and included via `#[path]`
//! (org convention), so they sit beside the code and reach internals via
//! `use super::*` when needed.

use super::*;

#[test]
fn with_capacity_behaves_like_new_but_preallocates() {
    let mut t: Treap<i32, &'static str> = Treap::with_capacity(7, 128);
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    for i in 0..64 {
        t.insert(i, "v");
    }
    assert_eq!(t.len(), 64);
    assert_eq!(t.get(&40).copied(), Some("v"));
    let ordered: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    assert!(ordered.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn insert_get_remove_round_trip() {
    let mut t: Treap<i32, &'static str> = Treap::new(7);
    t.insert(5, "five");
    t.insert(3, "three");
    t.insert(8, "eight");
    t.insert(1, "one");
    assert_eq!(t.len(), 4);
    assert_eq!(t.get(&5).copied(), Some("five"));
    assert_eq!(t.get(&3).copied(), Some("three"));
    assert_eq!(t.get(&8).copied(), Some("eight"));
    assert_eq!(t.get(&1).copied(), Some("one"));
    assert_eq!(t.get(&999), None);

    assert_eq!(t.remove(&3), Some("three"));
    assert_eq!(t.len(), 3);
    assert_eq!(t.get(&3), None);
    assert_eq!(t.remove(&3), None);
}

#[test]
fn insert_existing_key_replaces_value() {
    let mut t: Treap<i32, &'static str> = Treap::new(7);
    t.insert(1, "first");
    assert_eq!(t.insert(1, "second"), Some("first"));
    assert_eq!(t.len(), 1);
    assert_eq!(t.get(&1).copied(), Some("second"));
}

#[test]
fn in_order_traversal_is_sorted() {
    let mut t: Treap<i32, i32> = Treap::new(123);
    for k in [5, 1, 9, 3, 7, 2, 8] {
        t.insert(k, k * 10);
    }
    let keys: Vec<_> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![1, 2, 3, 5, 7, 8, 9]);
}

#[test]
fn supports_thousand_random_keys() {
    let mut t: Treap<u32, u32> = Treap::new(99);
    let mut x = 12345u32;
    let mut keys = Vec::new();
    for _ in 0..1_000 {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        keys.push(x);
        t.insert(x, x);
    }
    for k in &keys {
        assert_eq!(t.get(k).copied(), Some(*k));
    }
    assert_eq!(
        t.len(),
        keys.iter().collect::<std::collections::HashSet<_>>().len()
    );
}

#[test]
fn empty_treap_state() {
    let t: Treap<i32, &'static str> = Treap::new(0);
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    assert!(t.get(&1).is_none());
    assert!(t.collect_in_order().is_empty());
}

#[test]
fn remove_from_empty_returns_none() {
    let mut t: Treap<i32, &'static str> = Treap::new(0);
    assert!(t.remove(&1).is_none());
}

#[test]
fn ascending_inserts() {
    let mut t: Treap<i32, i32> = Treap::new(5);
    for i in 0..100 {
        t.insert(i, i * 2);
    }
    for i in 0..100 {
        assert_eq!(t.get(&i).copied(), Some(i * 2));
    }
    let keys: Vec<_> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    for w in keys.windows(2) {
        assert!(w[0] < w[1]);
    }
}

#[test]
fn descending_inserts() {
    let mut t: Treap<i32, i32> = Treap::new(5);
    for i in (0..100).rev() {
        t.insert(i, i * 2);
    }
    let keys: Vec<_> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    for w in keys.windows(2) {
        assert!(w[0] < w[1]);
    }
}

#[test]
fn remove_all_keys_one_by_one() {
    let mut t: Treap<i32, &'static str> = Treap::new(7);
    let n = 200;
    for i in 0..n {
        t.insert(i, "x");
    }
    assert_eq!(t.len(), n as usize);
    for i in 0..n {
        assert_eq!(t.remove(&i), Some("x"));
        assert!(t.get(&i).is_none());
    }
    assert!(t.is_empty());
}

#[test]
fn removed_slots_are_reused_by_later_inserts() {
    let mut t: Treap<i32, i32> = Treap::new(3);
    for i in 0..16 {
        t.insert(i, i);
    }
    let cap_after_fill = t.nodes.len();
    for i in 0..16 {
        assert_eq!(t.remove(&i), Some(i));
    }
    assert!(t.is_empty());
    // Re-inserting must draw from the free list, not grow the backing Vec.
    for i in 100..116 {
        t.insert(i, i);
    }
    assert_eq!(t.len(), 16);
    assert_eq!(t.nodes.len(), cap_after_fill, "freed slots reused");
    for i in 100..116 {
        assert_eq!(t.get(&i).copied(), Some(i));
    }
}

#[test]
fn interleaved_insert_remove() {
    let mut t: Treap<i32, i32> = Treap::new(11);
    for i in 0..50 {
        t.insert(i, i);
    }
    for i in 0..50 {
        if i % 2 == 0 {
            t.remove(&i);
        }
    }
    for i in 0..50 {
        if i % 2 == 0 {
            assert!(t.get(&i).is_none());
        } else {
            assert_eq!(t.get(&i).copied(), Some(i));
        }
    }
}
