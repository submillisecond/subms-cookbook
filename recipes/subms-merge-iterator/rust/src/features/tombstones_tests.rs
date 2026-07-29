use super::*;

fn live(k: &str, v: &str) -> TombstoneEntry<String, String> {
    TombstoneEntry::live(k.to_string(), v.to_string())
}
fn tomb(k: &str) -> TombstoneEntry<String, String> {
    TombstoneEntry::tombstone(k.to_string())
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
