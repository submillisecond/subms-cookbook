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
mod tests {
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
        let merged: Vec<_> = PriorityMergeIterator::new([
            src(10, vec![e("k", "high")]),
            src(1, vec![e("k", "low")]),
        ])
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
        let merged: Vec<_> = PriorityMergeIterator::new([
            src(0, vec![e("k", "zero")]),
            src(-100, vec![e("k", "neg")]),
        ])
        .collect();
        assert_eq!(merged, vec![e("k", "zero")]);
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
}
