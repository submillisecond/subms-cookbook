use super::*;

fn build(seed: u64, keys: &[i32]) -> SplittableTreap<i32, i32> {
    let mut t = SplittableTreap::new(seed);
    for &k in keys {
        t.insert(k, k * 10);
    }
    t
}

#[test]
fn empty_split_yields_two_empties() {
    let t: SplittableTreap<i32, i32> = SplittableTreap::new(0);
    let (lo, hi) = t.split(&5);
    assert!(lo.is_empty());
    assert!(hi.is_empty());
}

#[test]
fn single_node_split_below_pivot() {
    let t = build(0, &[5]);
    let (lo, hi) = t.split(&10);
    assert_eq!(lo.len(), 1);
    assert!(hi.is_empty());
    assert_eq!(lo.get(&5).copied(), Some(50));
}

#[test]
fn single_node_split_above_pivot() {
    let t = build(0, &[5]);
    let (lo, hi) = t.split(&1);
    assert!(lo.is_empty());
    assert_eq!(hi.len(), 1);
    assert_eq!(hi.get(&5).copied(), Some(50));
}

#[test]
fn split_at_existing_key_puts_key_on_right() {
    let t = build(7, &[1, 2, 3, 4, 5]);
    let (lo, hi) = t.split(&3);
    let lo_keys: Vec<i32> = lo.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    let hi_keys: Vec<i32> = hi.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    assert_eq!(lo_keys, vec![1, 2]);
    assert_eq!(hi_keys, vec![3, 4, 5]);
}

#[test]
fn split_then_merge_round_trips() {
    let t = build(7, &[5, 1, 9, 3, 7, 2, 8, 4, 6]);
    let original: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    let (lo, hi) = t.split(&5);
    let merged = SplittableTreap::merge(lo, hi);
    let after: Vec<i32> = merged
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(after, original);
    assert_eq!(merged.len(), 9);
}

#[test]
fn merge_disjoint_treaps_preserves_order() {
    let lo = build(7, &[1, 2, 3]);
    let hi = build(11, &[4, 5, 6]);
    let merged = SplittableTreap::merge(lo, hi);
    let keys: Vec<i32> = merged
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(keys, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(merged.len(), 6);
}

#[test]
fn merge_with_empty_left_returns_right() {
    let lo: SplittableTreap<i32, i32> = SplittableTreap::new(0);
    let hi = build(7, &[1, 2, 3]);
    let merged = SplittableTreap::merge(lo, hi);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged.get(&2).copied(), Some(20));
}

#[test]
fn merge_with_empty_right_returns_left() {
    let lo = build(7, &[1, 2, 3]);
    let hi: SplittableTreap<i32, i32> = SplittableTreap::new(0);
    let merged = SplittableTreap::merge(lo, hi);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged.get(&2).copied(), Some(20));
}

#[test]
fn split_at_pivot_below_all_keys() {
    let t = build(7, &[10, 20, 30]);
    let (lo, hi) = t.split(&5);
    assert!(lo.is_empty());
    assert_eq!(hi.len(), 3);
}

#[test]
fn split_at_pivot_above_all_keys() {
    let t = build(7, &[10, 20, 30]);
    let (lo, hi) = t.split(&100);
    assert_eq!(lo.len(), 3);
    assert!(hi.is_empty());
}

#[test]
fn insert_duplicate_replaces_value_and_returns_old() {
    let mut t = SplittableTreap::new(7);
    assert_eq!(t.insert(3, 30), None);
    assert_eq!(t.insert(1, 10), None);
    assert_eq!(t.len(), 2);
    let old = t.insert(3, 99);
    assert_eq!(old, Some(30));
    assert_eq!(t.len(), 2); // replace does not grow
    assert_eq!(t.get(&3).copied(), Some(99));
}

#[test]
fn get_absent_key_is_none() {
    let t = build(7, &[1, 2, 3]);
    assert!(t.get(&42).is_none());
    let empty: SplittableTreap<i32, i32> = SplittableTreap::new(0);
    assert!(empty.get(&1).is_none());
}

#[test]
fn large_split_merge_round_trip() {
    let mut t = SplittableTreap::new(42);
    for i in 0..500 {
        t.insert(i, i);
    }
    let original: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| *k).collect();
    let (lo, hi) = t.split(&250);
    assert_eq!(lo.len(), 250);
    assert_eq!(hi.len(), 250);
    let merged = SplittableTreap::merge(lo, hi);
    let after: Vec<i32> = merged
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(after, original);
}
