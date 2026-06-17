//! Tombstone-aware k-way merge. Entries carry a key, an optional value,
//! and a tombstone flag. When two or more sources produce entries with
//! the same key, the highest-indexed source ("latest") wins. If the
//! winning entry is a tombstone, the key is dropped from the output;
//! otherwise its `value` is yielded.
//!
//! This is the classic LSM-tree compaction shape: newer levels' delete
//! markers shadow older same-key writes.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A keyed entry. `value` is `None` for a tombstone (the entry exists
/// to mark the key as deleted), `Some(_)` for a live value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TombstoneEntry<K, V> {
    pub key: K,
    pub value: Option<V>,
}

impl<K, V> TombstoneEntry<K, V> {
    pub fn live(key: K, value: V) -> Self {
        Self {
            key,
            value: Some(value),
        }
    }
    pub fn tombstone(key: K) -> Self {
        Self { key, value: None }
    }
    pub fn is_tombstone(&self) -> bool {
        self.value.is_none()
    }
}

pub struct TombstoneMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = TombstoneEntry<K, V>>,
{
    streams: Vec<I>,
    heap: BinaryHeap<Reverse<HeapItem<K, V>>>,
}

// (key, source_index, value). Ordering: by key asc, then by source
// index DESC (higher source = "newer" pops first). Storing the key/
// value alongside keeps the heap allocation-free per `next()` call.
struct HeapItem<K, V> {
    key: K,
    source: usize,
    value: Option<V>,
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
        // Heap is min via Reverse(...) at the outer layer. Key asc,
        // then source DESC so that on key tie the newer source pops
        // off the min-heap first.
        self.key
            .cmp(&other.key)
            .then(other.source.cmp(&self.source))
    }
}

impl<K, V, I> TombstoneMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = TombstoneEntry<K, V>>,
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
}

impl<K, V, I> Iterator for TombstoneMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = TombstoneEntry<K, V>>,
{
    type Item = TombstoneEntry<K, V>;

    fn next(&mut self) -> Option<TombstoneEntry<K, V>> {
        loop {
            let Reverse(HeapItem {
                key: winning_key,
                source,
                value: winning_value,
            }) = self.heap.pop()?;
            self.advance(source);

            // Drop every other entry with the same key. The min-heap
            // ordering guarantees they pop next; we only need to peek
            // and skip while keys match.
            while let Some(Reverse(item)) = self.heap.peek() {
                if item.key == winning_key {
                    let Reverse(item) = self.heap.pop().unwrap();
                    self.advance(item.source);
                } else {
                    break;
                }
            }

            if winning_value.is_none() {
                // Tombstone wins - the key is masked, loop to find the
                // next distinct key.
                continue;
            }
            return Some(TombstoneEntry {
                key: winning_key,
                value: winning_value,
            });
        }
    }
}

impl<K, V, I> TombstoneMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = TombstoneEntry<K, V>>,
{
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

#[cfg(test)]
mod tests {
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
        let merged: Vec<_> =
            TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
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
        let merged: Vec<_> =
            TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
        assert!(merged.is_empty());
    }

    #[test]
    fn all_tombstones_yields_nothing() {
        let s0 = vec![tomb("a"), tomb("b")];
        let s1 = vec![tomb("a"), tomb("c")];
        let merged: Vec<_> =
            TombstoneMergeIterator::new([s0.into_iter(), s1.into_iter()]).collect();
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
}
