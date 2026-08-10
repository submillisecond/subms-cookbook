use super::*;

fn build_treap(keys: &[i32]) -> Treap<i32, i32> {
    let mut t: Treap<i32, i32> = Treap::new(42);
    for &k in keys {
        t.insert(k, k * 10);
    }
    t
}

fn collect_range<'a>(
    t: &'a Treap<i32, i32>,
    from: RangeBound<'a, i32>,
    to: RangeBound<'a, i32>,
) -> Vec<(i32, i32)> {
    t.range(from, to).map(|(k, v)| (*k, *v)).collect()
}

#[test]
fn empty_treap_yields_nothing() {
    let t: Treap<i32, i32> = Treap::new(0);
    let out = collect_range(&t, RangeBound::Unbounded, RangeBound::Unbounded);
    assert!(out.is_empty());
}

#[test]
fn single_node_inclusive_match() {
    let t = build_treap(&[5]);
    let out = collect_range(&t, RangeBound::Inclusive(&5), RangeBound::Inclusive(&5));
    assert_eq!(out, vec![(5, 50)]);
}

#[test]
fn single_node_exclusive_misses() {
    let t = build_treap(&[5]);
    let out = collect_range(&t, RangeBound::Exclusive(&5), RangeBound::Inclusive(&100));
    assert!(out.is_empty());
}

#[test]
fn inclusive_bounds_yield_sorted_window() {
    let t = build_treap(&[5, 1, 9, 3, 7, 2, 8, 4, 6]);
    let out = collect_range(&t, RangeBound::Inclusive(&3), RangeBound::Inclusive(&7));
    let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![3, 4, 5, 6, 7]);
}

#[test]
fn exclusive_bounds_drop_endpoints() {
    let t = build_treap(&[5, 1, 9, 3, 7, 2, 8, 4, 6]);
    let out = collect_range(&t, RangeBound::Exclusive(&3), RangeBound::Exclusive(&7));
    let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![4, 5, 6]);
}

#[test]
fn unbounded_below_iterates_from_min() {
    let t = build_treap(&[5, 1, 9, 3, 7]);
    let out = collect_range(&t, RangeBound::Unbounded, RangeBound::Inclusive(&5));
    let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![1, 3, 5]);
}

#[test]
fn unbounded_above_iterates_to_max() {
    let t = build_treap(&[5, 1, 9, 3, 7]);
    let out = collect_range(&t, RangeBound::Inclusive(&5), RangeBound::Unbounded);
    let keys: Vec<i32> = out.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![5, 7, 9]);
}

#[test]
fn range_outside_keys_yields_nothing() {
    let t = build_treap(&[10, 20, 30]);
    let out = collect_range(&t, RangeBound::Inclusive(&100), RangeBound::Inclusive(&200));
    assert!(out.is_empty());
}

#[test]
fn values_match_keys_in_range() {
    let t = build_treap(&[1, 2, 3, 4, 5]);
    let out = collect_range(&t, RangeBound::Inclusive(&2), RangeBound::Inclusive(&4));
    assert_eq!(out, vec![(2, 20), (3, 30), (4, 40)]);
}

#[test]
fn large_treap_in_order_invariant() {
    let mut t: Treap<i32, i32> = Treap::new(99);
    for i in 0..1_000 {
        t.insert(i, i);
    }
    let out: Vec<i32> = t
        .range(RangeBound::Inclusive(&100), RangeBound::Inclusive(&899))
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(out.len(), 800);
    for w in out.windows(2) {
        assert!(w[0] < w[1]);
    }
    assert_eq!(*out.first().unwrap(), 100);
    assert_eq!(*out.last().unwrap(), 899);
}
