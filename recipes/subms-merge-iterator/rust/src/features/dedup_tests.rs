use super::*;

fn e(k: &str, v: &str) -> DedupEntry<String, String> {
    DedupEntry::new(k.to_string(), v.to_string())
}

#[test]
fn distinct_keys_pass_through() {
    let s0 = vec![e("a", "1"), e("c", "3")];
    let s1 = vec![e("b", "2"), e("d", "4")];
    let merged: Vec<_> = DedupMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert_eq!(
        merged,
        vec![e("a", "1"), e("b", "2"), e("c", "3"), e("d", "4")]
    );
}

#[test]
fn duplicate_key_picks_latest_source() {
    let s0 = vec![e("k", "old")];
    let s1 = vec![e("k", "new")];
    let merged: Vec<_> = DedupMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert_eq!(merged, vec![e("k", "new")]);
}

#[test]
fn three_way_duplicate_picks_highest_source() {
    let s0 = vec![e("k", "v0")];
    let s1 = vec![e("k", "v1")];
    let s2 = vec![e("k", "v2")];
    let merged: Vec<_> =
        DedupMergeIterator::new([s0.into_iter(), s1.into_iter(), s2.into_iter()]).collect();
    assert_eq!(merged, vec![e("k", "v2")]);
}

#[test]
fn empty_sources_yield_empty() {
    let s0: Vec<DedupEntry<String, String>> = vec![];
    let s1: Vec<DedupEntry<String, String>> = vec![];
    let merged: Vec<_> = DedupMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert!(merged.is_empty());
}

#[test]
fn interleaved_with_some_duplicates() {
    let s0 = vec![e("a", "a0"), e("b", "b0"), e("c", "c0")];
    let s1 = vec![e("b", "b1"), e("d", "d1")];
    let merged: Vec<_> = DedupMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert_eq!(
        merged,
        vec![e("a", "a0"), e("b", "b1"), e("c", "c0"), e("d", "d1")]
    );
}

#[test]
fn dedup_preserves_count_with_unique_keys() {
    let s0: Vec<_> = (0..100i32).map(|i| DedupEntry::new(i * 2, i)).collect();
    let s1: Vec<_> = (0..100i32).map(|i| DedupEntry::new(i * 2 + 1, i)).collect();
    let merged: Vec<_> = DedupMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
    assert_eq!(merged.len(), 200);
    for w in merged.windows(2) {
        assert!(w[0].key < w[1].key);
    }
}

#[test]
fn all_duplicates_collapses_to_one_per_key() {
    // Three sources, each carrying the same five keys.
    let mk = |tag: &str| {
        (0..5i32)
            .map(move |k| DedupEntry::new(k, format!("{tag}-{k}")))
            .collect::<Vec<_>>()
    };
    let merged: Vec<_> = DedupMergeIterator::new([
        mk("a").into_iter(),
        mk("b").into_iter(),
        mk("c").into_iter(),
    ])
    .collect();
    // Latest source (c) wins for every key.
    assert_eq!(
        merged,
        (0..5i32)
            .map(|k| DedupEntry::new(k, format!("c-{k}")))
            .collect::<Vec<_>>()
    );
}
