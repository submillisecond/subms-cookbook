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
#[path = "tombstones_tests.rs"]
mod tests;
