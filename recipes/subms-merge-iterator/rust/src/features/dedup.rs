//! Deduplicating k-way merge. Each input is a stream of (key, value)
//! entries sorted by key. On key tie across sources, the highest-
//! indexed source wins ("latest"). Output is one entry per distinct
//! key.
//!
//! Same shape as the tombstone-aware merge, minus the tombstone flag -
//! every entry is "live". Useful for log compaction over append-only
//! shards where you want the freshest value per key.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedupEntry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> DedupEntry<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }
}

pub struct DedupMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = DedupEntry<K, V>>,
{
    streams: Vec<I>,
    heap: BinaryHeap<Reverse<HeapItem<K, V>>>,
}

struct HeapItem<K, V> {
    key: K,
    source: usize,
    value: V,
}

impl<K: Ord, V> PartialEq for HeapItem<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.source == other.source
    }
}
impl<K: Ord, V> Eq for HeapItem<K, V> {}

impl<K: Ord, V> PartialOrd for HeapItem<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<K: Ord, V> Ord for HeapItem<K, V> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key
            .cmp(&other.key)
            .then(other.source.cmp(&self.source))
    }
}

impl<K, V, I> DedupMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = DedupEntry<K, V>>,
{
    pub fn new<S: IntoIterator<Item = I>>(streams: S) -> Self {
        let mut streams: Vec<I> = streams.into_iter().collect();
        let mut heap = BinaryHeap::with_capacity(streams.len());
        for (i, s) in streams.iter_mut().enumerate() {
            if let Some(e) = s.next() {
                heap.push(Reverse(HeapItem {
                    key: e.key,
                    source: i,
                    value: e.value,
                }));
            }
        }
        Self { streams, heap }
    }

    fn advance(&mut self, source: usize) {
        if let Some(e) = self.streams[source].next() {
            self.heap.push(Reverse(HeapItem {
                key: e.key,
                source,
                value: e.value,
            }));
        }
    }
}

impl<K, V, I> Iterator for DedupMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = DedupEntry<K, V>>,
{
    type Item = DedupEntry<K, V>;

    fn next(&mut self) -> Option<DedupEntry<K, V>> {
        let Reverse(HeapItem {
            key: winning_key,
            source,
            value: winning_value,
        }) = self.heap.pop()?;
        self.advance(source);

        // Drop every other entry with the same key.
        while let Some(Reverse(item)) = self.heap.peek() {
            if item.key == winning_key {
                let Reverse(item) = self.heap.pop().unwrap();
                self.advance(item.source);
            } else {
                break;
            }
        }
        Some(DedupEntry {
            key: winning_key,
            value: winning_value,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
