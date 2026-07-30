use super::*;

fn e(k: &str, v: &str) -> PriorityEntry<String, String> {
    PriorityEntry::new(k.to_string(), v.to_string())
}

fn src(
    prio: i32,
    entries: Vec<PriorityEntry<String, String>>,
) -> PrioritySource<std::vec::IntoIter<PriorityEntry<String, String>>> {
    PrioritySource::new(prio, entries.into_iter())
}

#[test]
fn higher_priority_wins_on_tie() {
    // Source 0 has prio 10 (high), source 1 has prio 1 (low),
    // even though source 1 is registered later.
    let merged: Vec<_> =
        PriorityMergeIterator::new([src(10, vec![e("k", "high")]), src(1, vec![e("k", "low")])])
            .collect();
    assert_eq!(merged, vec![e("k", "high")]);
}

#[test]
fn equal_priority_falls_back_to_registration_order() {
    // Both prio 5 -> latest registered (source 1) wins.
    let merged: Vec<_> = PriorityMergeIterator::new([
        src(5, vec![e("k", "first")]),
        src(5, vec![e("k", "second")]),
    ])
    .collect();
    assert_eq!(merged, vec![e("k", "second")]);
}

#[test]
fn three_sources_priority_tie_break() {
    // Prios 1, 100, 50. Highest is the middle source.
    let merged: Vec<_> = PriorityMergeIterator::new([
        src(1, vec![e("k", "p1")]),
        src(100, vec![e("k", "p100")]),
        src(50, vec![e("k", "p50")]),
    ])
    .collect();
    assert_eq!(merged, vec![e("k", "p100")]);
}

#[test]
fn distinct_keys_yield_every_entry() {
    let merged: Vec<_> = PriorityMergeIterator::new([
        src(10, vec![e("a", "a-hi"), e("c", "c-hi")]),
        src(1, vec![e("b", "b-lo"), e("d", "d-lo")]),
    ])
    .collect();
    assert_eq!(
        merged,
        vec![
            e("a", "a-hi"),
            e("b", "b-lo"),
            e("c", "c-hi"),
            e("d", "d-lo")
        ]
    );
}

#[test]
fn empty_sources_yield_empty() {
    let merged: Vec<_> = PriorityMergeIterator::new(Vec::<
        PrioritySource<std::vec::IntoIter<PriorityEntry<String, String>>>,
    >::new())
    .collect();
    assert!(merged.is_empty());
}

#[test]
fn negative_priority_loses_to_zero() {
    let merged: Vec<_> =
        PriorityMergeIterator::new([src(0, vec![e("k", "zero")]), src(-100, vec![e("k", "neg")])])
            .collect();
    assert_eq!(merged, vec![e("k", "zero")]);
}

#[test]
fn heap_item_eq_and_ord_direct() {
    let a = HeapItem {
        key: 5i32,
        priority: 10,
        source: 0,
        value: "a",
    };
    let b = HeapItem {
        key: 5i32,
        priority: 99,
        source: 0,
        value: "z",
    };
    let same = HeapItem {
        key: 5i32,
        priority: 10,
        source: 0,
        value: "x",
    };
    let hi_prio = HeapItem {
        key: 5i32,
        priority: 100,
        source: 3,
        value: "p",
    };
    let lo_prio = HeapItem {
        key: 5i32,
        priority: 1,
        source: 3,
        value: "q",
    };
    let bigger_key = HeapItem {
        key: 9i32,
        priority: 0,
        source: 0,
        value: "k",
    };
    // eq keyed on (key, source), ignoring priority + value.
    assert!(a == b);
    assert!(a != bigger_key);
    // Ord: key asc, then priority DESC, then source DESC.
    assert_eq!(a.cmp(&bigger_key), std::cmp::Ordering::Less);
    assert_eq!(hi_prio.cmp(&lo_prio), std::cmp::Ordering::Less);
    // Higher priority sorts Less (pops first off the min-heap).
    assert_eq!(a.cmp(&b), std::cmp::Ordering::Greater);
    assert_eq!(a.partial_cmp(&same), Some(std::cmp::Ordering::Equal));
}

// Multi-element streams so advance() re-pushes many times and the
// same-key drain (break arm) runs across many next() calls.
#[test]
fn long_streams_exercise_advance_and_drain() {
    // Zero-padded keys so lexicographic string order matches numeric
    // order (the merge requires each input sorted by key).
    let hi: Vec<_> = (0..25i32)
        .map(|i| e(&format!("{i:02}"), &format!("hi-{i}")))
        .collect();
    let lo: Vec<_> = (0..25i32)
        .map(|i| e(&format!("{i:02}"), &format!("lo-{i}")))
        .collect();
    let merged: Vec<_> = PriorityMergeIterator::new([src(100, hi), src(1, lo)]).collect();
    assert_eq!(merged.len(), 25);
    // High-priority source wins every colliding key.
    for (i, entry) in merged.iter().enumerate() {
        assert_eq!(entry.value, format!("hi-{i}"));
    }
}

#[test]
fn mixed_keys_and_priorities() {
    // src 0 (high): a, c, e
    // src 1 (low):  a, b, c (a and c collide with src 0)
    // Expected: a-hi, b-lo, c-hi, e-hi
    let merged: Vec<_> = PriorityMergeIterator::new([
        src(10, vec![e("a", "a-hi"), e("c", "c-hi"), e("e", "e-hi")]),
        src(1, vec![e("a", "a-lo"), e("b", "b-lo"), e("c", "c-lo")]),
    ])
    .collect();
    assert_eq!(
        merged,
        vec![
            e("a", "a-hi"),
            e("b", "b-lo"),
            e("c", "c-hi"),
            e("e", "e-hi")
        ]
    );
}
