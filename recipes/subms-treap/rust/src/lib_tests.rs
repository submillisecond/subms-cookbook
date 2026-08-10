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

// A vacated arena slot has had its payload moved out. Assigning over it must
// not run the old destructor, and the arena's own Drop must skip it. Before
// the ManuallyDrop rewrite both ran, which double-freed every `V: Drop`.
mod drop_accounting {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static DROPPED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    struct Tracked(u32);

    impl Drop for Tracked {
        fn drop(&mut self) {
            DROPPED.with(|d| d.borrow_mut().push(self.0));
        }
    }

    fn dropped() -> Vec<u32> {
        DROPPED.with(|d| d.borrow().clone())
    }

    #[test]
    fn every_value_drops_exactly_once() {
        DROPPED.with(|d| d.borrow_mut().clear());
        {
            let mut t: Treap<i32, Tracked> = Treap::new(7);
            t.insert(1, Tracked(1));
            t.insert(2, Tracked(2));
            drop(t.remove(&1).unwrap());
            assert_eq!(dropped(), vec![1]);
            // Reuses the slot just freed.
            t.insert(3, Tracked(3));
            assert_eq!(
                dropped(),
                vec![1],
                "slot reuse must not drop the old payload"
            );
        }
        let mut seen = dropped();
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2, 3]);
    }

    #[test]
    fn clear_drops_live_entries_once() {
        DROPPED.with(|d| d.borrow_mut().clear());
        let mut t: Treap<i32, Tracked> = Treap::new(3);
        for i in 0..8 {
            t.insert(i, Tracked(i as u32));
        }
        drop(t.remove(&4).unwrap());
        t.clear();
        assert!(t.is_empty());
        let mut seen = dropped();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        drop(t);
        let mut after = dropped();
        after.sort_unstable();
        assert_eq!(after.len(), 8, "clear left nothing for Drop to free again");
    }

    #[test]
    fn replaced_value_drops_once() {
        DROPPED.with(|d| d.borrow_mut().clear());
        let mut t: Treap<i32, Tracked> = Treap::new(9);
        t.insert(1, Tracked(10));
        let old = t.insert(1, Tracked(11)).expect("replaced");
        assert_eq!(old.0, 10);
        drop(old);
        assert_eq!(dropped(), vec![10]);
    }
}

#[test]
fn contains_key_matches_get() {
    let mut t: Treap<i32, i32> = Treap::new(21);
    for k in [4, 9, 1, 7] {
        t.insert(k, k);
    }
    for k in 0..12 {
        assert_eq!(t.contains_key(&k), t.get(&k).is_some());
    }
}

#[test]
fn get_mut_amends_in_place() {
    let mut t: Treap<u32, u64> = Treap::new(4);
    t.insert(10_000, 500);
    *t.get_mut(&10_000).unwrap() -= 200;
    assert_eq!(t.get(&10_000).copied(), Some(300));
    assert!(t.get_mut(&99).is_none());
    assert_eq!(t.len(), 1, "amend does not add a level");
}

#[test]
fn clear_resets_but_keeps_capacity() {
    let mut t: Treap<i32, i32> = Treap::with_capacity(6, 64);
    for i in 0..32 {
        t.insert(i, i);
    }
    let capacity = t.nodes.capacity();
    t.clear();
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    assert_eq!(t.height(), 0);
    assert!(t.get(&5).is_none());
    assert_eq!(
        t.nodes.capacity(),
        capacity,
        "arena capacity survives clear"
    );
    t.insert(5, 5);
    assert_eq!(t.get(&5).copied(), Some(5));
}

#[test]
fn first_last_and_pop_extremes() {
    let mut t: Treap<u32, u64> = Treap::new(13);
    assert!(t.first().is_none());
    assert!(t.last().is_none());
    assert!(t.pop_first().is_none());
    assert!(t.pop_last().is_none());

    for px in [9998u32, 10_001, 9999, 10_000] {
        t.insert(px, px as u64 * 2);
    }
    assert_eq!(t.first().map(|(k, _)| *k), Some(9998));
    assert_eq!(t.last().map(|(k, _)| *k), Some(10_001));

    assert_eq!(t.pop_first(), Some((9998, 19_996)));
    assert_eq!(t.pop_last(), Some((10_001, 20_002)));
    assert_eq!(t.len(), 2);
    let keys: Vec<u32> = t.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![9999, 10_000]);
}

#[test]
fn pop_drains_in_key_order() {
    let mut t: Treap<i32, i32> = Treap::new(17);
    for i in [5, 2, 9, 1, 7, 3] {
        t.insert(i, i);
    }
    let mut drained = Vec::new();
    while let Some((k, _)) = t.pop_first() {
        drained.push(k);
    }
    assert_eq!(drained, vec![1, 2, 3, 5, 7, 9]);
    assert!(t.is_empty());
}

#[test]
fn floor_ceiling_predecessor_successor() {
    let mut t: Treap<i32, i32> = Treap::new(31);
    for k in [10, 20, 30, 40] {
        t.insert(k, k);
    }
    assert_eq!(t.floor(&25).map(|(k, _)| *k), Some(20));
    assert_eq!(t.floor(&30).map(|(k, _)| *k), Some(30));
    assert_eq!(t.floor(&5), None);

    assert_eq!(t.ceiling(&25).map(|(k, _)| *k), Some(30));
    assert_eq!(t.ceiling(&30).map(|(k, _)| *k), Some(30));
    assert_eq!(t.ceiling(&41), None);

    assert_eq!(t.predecessor(&30).map(|(k, _)| *k), Some(20));
    assert_eq!(t.predecessor(&10), None);
    assert_eq!(t.successor(&30).map(|(k, _)| *k), Some(40));
    assert_eq!(t.successor(&40), None);
}

#[test]
fn navigation_agrees_with_a_sorted_scan() {
    let mut t: Treap<u32, u32> = Treap::new(77);
    let keys: Vec<u32> = (0..200).map(|i| i * 3 + 1).collect();
    // Insert every key, in a scrambled order, so the tree shape is not the
    // key order but the contents still match `keys` exactly.
    let mut shuffled = keys.clone();
    let mut x = 7u32;
    for i in (1..shuffled.len()).rev() {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        shuffled.swap(i, (x as usize) % (i + 1));
    }
    for k in &shuffled {
        t.insert(*k, *k);
    }
    assert_eq!(t.len(), keys.len());
    for probe in 0..600u32 {
        let expect_floor = keys.iter().rev().find(|k| **k <= probe).copied();
        let expect_ceiling = keys.iter().find(|k| **k >= probe).copied();
        let expect_pred = keys.iter().rev().find(|k| **k < probe).copied();
        let expect_succ = keys.iter().find(|k| **k > probe).copied();
        assert_eq!(
            t.floor(&probe).map(|(k, _)| *k),
            expect_floor,
            "floor {probe}"
        );
        assert_eq!(
            t.ceiling(&probe).map(|(k, _)| *k),
            expect_ceiling,
            "ceiling {probe}"
        );
        assert_eq!(
            t.predecessor(&probe).map(|(k, _)| *k),
            expect_pred,
            "predecessor {probe}"
        );
        assert_eq!(
            t.successor(&probe).map(|(k, _)| *k),
            expect_succ,
            "successor {probe}"
        );
    }
}

#[test]
fn iteration_runs_both_directions() {
    let mut t: Treap<i32, i32> = Treap::new(41);
    for k in [5, 1, 9, 3, 7] {
        t.insert(k, k * 10);
    }
    let up: Vec<(i32, i32)> = t.iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(up, vec![(1, 10), (3, 30), (5, 50), (7, 70), (9, 90)]);
    let down: Vec<i32> = t.iter_rev().map(|(k, _)| *k).collect();
    assert_eq!(down, vec![9, 7, 5, 3, 1]);

    let via_trait: Vec<i32> = (&t).into_iter().map(|(k, _)| *k).collect();
    assert_eq!(via_trait, vec![1, 3, 5, 7, 9]);

    let empty: Treap<i32, i32> = Treap::new(0);
    assert_eq!(empty.iter().count(), 0);
    assert_eq!(empty.iter_rev().count(), 0);
}

#[test]
fn from_sorted_round_trips_a_collected_snapshot() {
    let mut source: Treap<u32, u64> = Treap::new(55);
    for i in 0..500u32 {
        source.insert(i * 7, i as u64);
    }
    let snapshot: Vec<(u32, u64)> = source.iter().map(|(k, v)| (*k, *v)).collect();

    let rebuilt = Treap::from_sorted(55, snapshot.clone()).expect("strictly ascending");
    assert_eq!(rebuilt.len(), source.len());
    let round_tripped: Vec<(u32, u64)> = rebuilt.iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(round_tripped, snapshot);
    for (k, v) in &snapshot {
        assert_eq!(rebuilt.get(k).copied(), Some(*v));
    }
    // The bulk build must produce a real treap, not a right spine.
    assert!(
        rebuilt.height() < 40,
        "bulk build stayed balanced, height {}",
        rebuilt.height()
    );
}

#[test]
fn from_sorted_rejects_unsorted_and_duplicate_input() {
    let out_of_order = Treap::from_sorted(1, [(1u32, "a"), (3, "b"), (2, "c")]);
    assert_eq!(
        out_of_order.unwrap_err(),
        TreapError::UnsortedInput { index: 2 }
    );

    let duplicate = Treap::from_sorted(1, [(1u32, "a"), (1, "b")]);
    assert_eq!(
        duplicate.unwrap_err(),
        TreapError::UnsortedInput { index: 1 }
    );

    let empty: Treap<u32, u32> = Treap::from_sorted(1, []).expect("empty input is sorted");
    assert!(empty.is_empty());
    assert_eq!(empty.height(), 0);
}

#[test]
fn treap_error_renders_its_index() {
    let err = TreapError::UnsortedInput { index: 9 };
    assert!(format!("{err}").contains("index 9"));
    assert!(format!("{err:?}").contains("UnsortedInput"));
    let _: &dyn std::error::Error = &err;
}

#[test]
fn from_sorted_keeps_the_heap_invariant() {
    let items: Vec<(u32, u32)> = (0..1_000u32).map(|i| (i, i)).collect();
    let t = Treap::from_sorted(123, items).unwrap();
    // Walk every node; a parent must outrank both children.
    let mut stack = vec![t.root];
    while let Some(idx) = stack.pop() {
        if idx == NIL {
            continue;
        }
        let node = &t.nodes[idx as usize];
        for child in [node.left, node.right] {
            if child != NIL {
                assert!(
                    t.nodes[child as usize].priority <= node.priority,
                    "max-heap on priority"
                );
                stack.push(child);
            }
        }
    }
}

#[test]
fn height_tracks_the_randomized_bound() {
    // Deterministic: fixed seed, fixed key stream. The randomized-priority
    // bound is ~3*ln(n) expected; the assertion is loose enough to be a
    // regression guard on the priority stream, not a test of the constant.
    let n = 20_000u32;
    let mut t: Treap<u32, u32> = Treap::with_capacity(2024, n as usize);
    let mut x = 0xDEECE66Du32;
    for _ in 0..n {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        t.insert(x, x);
    }
    let expected = 3.0 * (t.len() as f64).ln();
    let height = t.height() as f64;
    assert!(
        height < 2.0 * expected,
        "height {height} against 3*ln(n) = {expected:.1} - priority stream degraded?"
    );
    assert!(
        height >= (t.len() as f64).log2() - 1.0,
        "height below the floor"
    );
}

#[test]
fn ascending_keys_do_not_build_a_spine() {
    // The failure mode the SplitMix64 finalizer exists to prevent: a key
    // stream correlated with the priority stream degenerating to O(n) depth.
    let n = 20_000i32;
    let mut t: Treap<i32, i32> = Treap::with_capacity(1, n as usize);
    for i in 0..n {
        t.insert(i, i);
    }
    let expected = 3.0 * (n as f64).ln();
    assert!(
        (t.height() as f64) < 2.0 * expected,
        "ascending inserts stayed logarithmic, height {}",
        t.height()
    );
}

#[test]
fn from_entropy_still_builds_a_balanced_tree() {
    // Output is not reproducible by construction, so the assertion is on the
    // shape, not on a value.
    let mut t: Treap<u32, u32> = Treap::from_entropy();
    for i in 0..4_096u32 {
        t.insert(i, i);
    }
    assert_eq!(t.len(), 4_096);
    assert!((t.height() as f64) < 6.0 * (4_096f64).ln());
    assert_eq!(t.first().map(|(k, _)| *k), Some(0));
}

#[test]
fn split_off_cuts_at_the_pivot() {
    let mut book: Treap<u32, u64> = Treap::new(19);
    for px in 9_990..10_010u32 {
        book.insert(px, px as u64);
    }
    let marketable = book.split_off(&10_000);
    assert_eq!(book.len(), 10);
    assert_eq!(marketable.len(), 10);
    assert_eq!(book.last().map(|(k, _)| *k), Some(9_999));
    assert_eq!(marketable.first().map(|(k, _)| *k), Some(10_000));
    assert!(book.iter().all(|(k, _)| *k < 10_000));
    assert!(marketable.iter().all(|(k, _)| *k >= 10_000));
    // Both halves are still usable treaps, not detached node soup.
    assert_eq!(marketable.get(&10_005).copied(), Some(10_005));
    assert!(marketable.height() >= 1);
}

#[test]
fn split_off_handles_the_degenerate_pivots() {
    let mut t: Treap<i32, i32> = Treap::new(23);
    for k in 0..8 {
        t.insert(k, k);
    }
    let all = t.split_off(&-1);
    assert!(t.is_empty(), "pivot below every key takes the whole tree");
    assert_eq!(all.len(), 8);

    let mut t2: Treap<i32, i32> = Treap::new(23);
    for k in 0..8 {
        t2.insert(k, k);
    }
    let none = t2.split_off(&100);
    assert_eq!(t2.len(), 8);
    assert!(none.is_empty());

    let mut empty: Treap<i32, i32> = Treap::new(1);
    assert!(empty.split_off(&0).is_empty());
}

#[test]
fn split_then_join_round_trips() {
    let mut book: Treap<u32, u64> = Treap::new(29);
    for px in 9_990..10_010u32 {
        book.insert(px, px as u64 * 2);
    }
    let before: Vec<(u32, u64)> = book.iter().map(|(k, v)| (*k, *v)).collect();

    let upper = book.split_off(&10_000);
    book.join(upper).expect("disjoint halves");
    assert_eq!(book.len(), 20);
    assert_eq!(
        book.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
        before
    );
    // The rejoined tree keeps the heap invariant, so later ops stay logarithmic.
    let mut stack = vec![book.root];
    while let Some(idx) = stack.pop() {
        if idx == NIL {
            continue;
        }
        let node = &book.nodes[idx as usize];
        for child in [node.left, node.right] {
            if child != NIL {
                assert!(book.nodes[child as usize].priority <= node.priority);
                stack.push(child);
            }
        }
    }
}

#[test]
fn join_refuses_overlapping_ranges() {
    let mut lo: Treap<i32, i32> = Treap::new(31);
    for k in 0..5 {
        lo.insert(k, k);
    }
    let mut overlapping: Treap<i32, i32> = Treap::new(37);
    for k in 3..8 {
        overlapping.insert(k, k);
    }
    assert_eq!(lo.join(overlapping), Err(TreapError::OverlappingRange));
    assert_eq!(lo.len(), 5, "refused join left the receiver untouched");

    let empty: Treap<i32, i32> = Treap::new(41);
    lo.join(empty)
        .expect("joining an empty treap is always legal");
    assert_eq!(lo.len(), 5);

    let mut fresh: Treap<i32, i32> = Treap::new(43);
    let mut donor: Treap<i32, i32> = Treap::new(47);
    for k in 0..4 {
        donor.insert(k, k);
    }
    fresh.join(donor).expect("empty receiver takes everything");
    assert_eq!(
        fresh.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn split_off_reuses_the_vacated_slots() {
    let mut t: Treap<i32, i32> = Treap::new(53);
    for k in 0..64 {
        t.insert(k, k);
    }
    let arena_before = t.nodes.len();
    let upper = t.split_off(&32);
    assert_eq!(upper.len(), 32);
    // The 32 relocated slots went on the free list rather than leaking.
    for k in 100..132 {
        t.insert(k, k);
    }
    assert_eq!(
        t.nodes.len(),
        arena_before,
        "split reuses the vacated slots"
    );
    assert_eq!(t.len(), 64);
}

// A relocated payload must move, not copy: the arena that gave it up has to
// forget it, or Drop runs twice.
#[test]
fn split_off_moves_payloads_exactly_once() {
    use std::cell::RefCell;
    thread_local! {
        static COUNT: RefCell<usize> = const { RefCell::new(0) };
    }
    struct Counted;
    impl Drop for Counted {
        fn drop(&mut self) {
            COUNT.with(|c| *c.borrow_mut() += 1);
        }
    }
    {
        let mut t: Treap<i32, Counted> = Treap::new(59);
        for k in 0..16 {
            t.insert(k, Counted);
        }
        let upper = t.split_off(&8);
        assert_eq!(COUNT.with(|c| *c.borrow()), 0, "relocation drops nothing");
        drop(upper);
        assert_eq!(COUNT.with(|c| *c.borrow()), 8);
    }
    assert_eq!(COUNT.with(|c| *c.borrow()), 16, "each payload dropped once");
}
