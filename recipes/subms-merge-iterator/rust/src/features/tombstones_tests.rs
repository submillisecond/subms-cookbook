use super::*;

fn live(k: &str, v: &str) -> TombstoneEntry<String, String> {
    TombstoneEntry::live(k.to_string(), v.to_string())
}
fn tomb(k: &str) -> TombstoneEntry<String, String> {
    TombstoneEntry::tombstone(k.to_string())
}

#[test]
fn entry_constructors_and_is_tombstone() {
    let l = live("k", "v");
    assert!(!l.is_tombstone());
    assert_eq!(l.value.as_deref(), Some("v"));
    let t = tomb("k");
    assert!(t.is_tombstone());
    assert_eq!(t.value, None);
}

#[test]
fn live_entries_passthrough_when_no_tombstones() {
    let s0 = vec![live("a", "1"), live("c", "3")];
    let s1 = vec![live("b", "2"), live("d", "4")];
    let merged: Vec<_> = TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert_eq!(
        merged,
        vec![
            live("a", "1"),
            live("b", "2"),
            live("c", "3"),
            live("d", "4")
        ]
    );
}

#[test]
fn later_source_tombstone_hides_earlier_value() {
    let older = vec![live("k", "v")];
    let newer = vec![tomb("k")];
    let merged: Vec<_> =
        TombstoneMergeIterator::new([older.into_iter(), newer.into_iter()]).collect();
    assert!(merged.is_empty(), "tombstone in newer source must hide k");
}

#[test]
fn later_source_live_overwrites_earlier_value() {
    let older = vec![live("k", "old")];
    let newer = vec![live("k", "new")];
    let merged: Vec<_> =
        TombstoneMergeIterator::new([older.into_iter(), newer.into_iter()]).collect();
    assert_eq!(merged, vec![live("k", "new")]);
}

#[test]
fn earlier_source_tombstone_does_not_hide_later_value() {
    // Source 0 (older) has tombstone, source 1 (newer) has live.
    // Newer wins -> live value yielded.
    let older = vec![tomb("k")];
    let newer = vec![live("k", "v")];
    let merged: Vec<_> =
        TombstoneMergeIterator::new([older.into_iter(), newer.into_iter()]).collect();
    assert_eq!(merged, vec![live("k", "v")]);
}

#[test]
fn tombstone_shadowing_spans_three_sources() {
    // src 0: live a, live b
    // src 1: tomb a
    // src 2: live c
    // Expected: b, c (a is killed by src 1's tombstone, src 2 has no a).
    let s0 = vec![live("a", "1"), live("b", "2")];
    let s1 = vec![tomb("a")];
    let s2 = vec![live("c", "3")];
    let merged: Vec<_> =
        TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter(), s2.into_iter()]).collect();
    assert_eq!(merged, vec![live("b", "2"), live("c", "3")]);
}

#[test]
fn all_sources_empty_yields_nothing() {
    let s0: Vec<TombstoneEntry<String, String>> = vec![];
    let s1: Vec<TombstoneEntry<String, String>> = vec![];
    let merged: Vec<_> = TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert!(merged.is_empty());
}

#[test]
fn all_tombstones_yields_nothing() {
    let s0 = vec![tomb("a"), tomb("b")];
    let s1 = vec![tomb("a"), tomb("c")];
    let merged: Vec<_> = TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert!(merged.is_empty());
}

#[test]
fn heap_item_eq_and_ord_direct() {
    let a: HeapItem<i32, &str> = HeapItem {
        key: 5,
        source: 0,
        value: Some("a"),
    };
    let b: HeapItem<i32, &str> = HeapItem {
        key: 5,
        source: 0,
        value: None,
    };
    let newer: HeapItem<i32, &str> = HeapItem {
        key: 5,
        source: 2,
        value: Some("a"),
    };
    let bigger: HeapItem<i32, &str> = HeapItem {
        key: 9,
        source: 0,
        value: Some("a"),
    };
    // eq keyed on (key, source), ignoring value.
    assert!(a == b);
    assert!(a != newer);
    // Ord: key asc, then source DESC (newer source pops first).
    assert_eq!(a.cmp(&bigger), std::cmp::Ordering::Less);
    assert_eq!(a.cmp(&newer), std::cmp::Ordering::Greater);
    assert_eq!(a.partial_cmp(&b), Some(std::cmp::Ordering::Equal));
}

// Long multi-source run mixing live values and tombstones so the
// tombstone-mask continue arm and the same-key drain break arm both
// run many times, alongside repeated advance() re-pushes.
#[test]
fn long_mixed_run_masks_and_drains() {
    // src 0 (oldest): live 0..30
    // src 1: tombstones for every even key 0..30
    // src 2 (newest): live for every key divisible by 3 in 0..30
    // Zero-padded keys so lexicographic order matches numeric order
    // (the merge requires each input sorted by key).
    let s0: Vec<_> = (0..30i32)
        .map(|k| live(&format!("{k:02}"), &format!("v{k}")))
        .collect();
    let s1: Vec<_> = (0..30i32)
        .filter(|k| k % 2 == 0)
        .map(|k| tomb(&format!("{k:02}")))
        .collect();
    let s2: Vec<_> = (0..30i32)
        .filter(|k| k % 3 == 0)
        .map(|k| live(&format!("{k:02}"), &format!("n{k}")))
        .collect();
    let merged: Vec<_> =
        TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter(), s2.into_iter()]).collect();
    // A key survives iff it is NOT masked by an even-key tombstone unless
    // a newer (src 2) live entry resurrects it. src 2 wins ties.
    for entry in &merged {
        let k: i32 = entry.key.parse().unwrap();
        assert!(!entry.is_tombstone());
        // Even keys only survive if src 2 (divisible by 3) resurrected them.
        if k % 2 == 0 {
            assert_eq!(
                k % 3,
                0,
                "even key {k} survived without a src-2 resurrection"
            );
        }
    }
    // Keys are strictly increasing.
    for w in merged.windows(2) {
        let a: i32 = w[0].key.parse().unwrap();
        let b: i32 = w[1].key.parse().unwrap();
        assert!(a < b);
    }
}

#[test]
fn interleaved_tombstones_and_live_resolve_per_key() {
    // src 0: live a, live b, live c
    // src 1: tomb a, live b (overwrites)
    // src 2: tomb b
    // Expected output keys: c only.
    let s0 = vec![live("a", "1"), live("b", "2"), live("c", "3")];
    let s1 = vec![tomb("a"), live("b", "new")];
    let s2 = vec![tomb("b")];
    let merged: Vec<_> =
        TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter(), s2.into_iter()]).collect();
    assert_eq!(merged, vec![live("c", "3")]);
}
