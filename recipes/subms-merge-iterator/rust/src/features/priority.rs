//! Priority-aware k-way merge. Each source carries an explicit
//! `priority` integer. On key tie, the highest-priority source wins.
//! Ties between equal priorities fall through to the source's
//! registration order (higher source index breaks the tie - matches
//! the latest-source-wins shape used by `dedup` and `tombstones`).
//!
//! This generalises `dedup`: pass `(priority = source_index, ...)` for
//! the same behaviour. The reason to reach for `priority` is when the
//! producer order on the wire doesn't match the recency / authority
//! ordering you want (e.g. an in-memory memtable should beat every
//! on-disk SSTable level even though it was registered first).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorityEntry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> PriorityEntry<K, V> {
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }
}

/// A source + the priority it carries. Higher `priority` wins on key
/// ties. Equal-priority ties fall through to registration order (the
/// index in the `new(...)` argument list).
pub struct PrioritySource<I> {
    pub priority: i32,
    pub stream: I,
}

impl<I> PrioritySource<I> {
    pub fn new(priority: i32, stream: I) -> Self {
        Self { priority, stream }
    }
}

pub struct PriorityMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = PriorityEntry<K, V>>,
{
    streams: Vec<I>,
    priorities: Vec<i32>,
    heap: BinaryHeap<Reverse<HeapItem<K, V>>>,
}

struct HeapItem<K, V> {
    key: K,
    /// Higher = wins. Negated so the min-heap pops it first on tie.
    priority: i32,
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
        // Key asc, then priority DESC (higher pops first off min-heap
        // via Reverse), then source DESC (latest registration wins on
        // priority tie).
        self.key
            .cmp(&other.key)
            .then(other.priority.cmp(&self.priority))
            .then(other.source.cmp(&self.source))
    }
}

impl<K, V, I> PriorityMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = PriorityEntry<K, V>>,
{
    pub fn new<S: IntoIterator<Item = PrioritySource<I>>>(sources: S) -> Self {
        let sources: Vec<PrioritySource<I>> = sources.into_iter().collect();
        let mut streams: Vec<I> = Vec::with_capacity(sources.len());
        let mut priorities: Vec<i32> = Vec::with_capacity(sources.len());
        let mut heap: BinaryHeap<Reverse<HeapItem<K, V>>> =
            BinaryHeap::with_capacity(sources.len());
        for (i, src) in sources.into_iter().enumerate() {
            streams.push(src.stream);
            priorities.push(src.priority);
            if let Some(e) = streams[i].next() {
                heap.push(Reverse(HeapItem {
                    key: e.key,
                    priority: src.priority,
                    source: i,
                    value: e.value,
                }));
            }
        }
        Self {
            streams,
            priorities,
            heap,
        }
    }

    fn advance(&mut self, source: usize) {
        if let Some(e) = self.streams[source].next() {
            let p = self.priorities[source];
            self.heap.push(Reverse(HeapItem {
                key: e.key,
                priority: p,
                source,
                value: e.value,
            }));
        }
    }
}

impl<K, V, I> Iterator for PriorityMergeIterator<K, V, I>
where
    K: Ord,
    I: Iterator<Item = PriorityEntry<K, V>>,
{
    type Item = PriorityEntry<K, V>;

    fn next(&mut self) -> Option<PriorityEntry<K, V>> {
        let Reverse(HeapItem {
            key: winning_key,
            source,
            value: winning_value,
            ..
        }) = self.heap.pop()?;
        self.advance(source);
        while let Some(Reverse(item)) = self.heap.peek() {
            if item.key == winning_key {
                let Reverse(item) = self.heap.pop().unwrap();
                self.advance(item.source);
            } else {
                break;
            }
        }
        Some(PriorityEntry {
            key: winning_key,
            value: winning_value,
        })
    }
}

#[cfg(test)]
#[path = "priority_tests.rs"]
mod tests;
