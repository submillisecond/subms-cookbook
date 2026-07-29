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
#[path = "dedup_tests.rs"]
mod tests;
