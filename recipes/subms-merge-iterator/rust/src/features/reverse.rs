//! Descending k-way merge - the backward half of a cursor. Sources must
//! be sorted DESCENDING; output is the global descending union.
//!
//! A freshly built `ReverseMergeIterator` sits on the largest value
//! across every source, which is the position RocksDB calls
//! `SeekToLast`. `seek_for_prev(target)` moves it to the largest value
//! `<= target`, and `set_lower_bound(lo)` stops the walk at `lo`
//! inclusive, matching RocksDB's `iterate_lower_bound`.
//!
//! What this is NOT is a bidirectional cursor. RocksDB's `Prev()` can
//! reverse mid-scan because each child is a seekable file cursor; the
//! sources here are one-shot iterators that only move forward through
//! their own order, so a direction flip would have to re-read them.
//! Pick the direction when you open the merge.

use std::collections::BinaryHeap;

pub struct ReverseMergeIterator<T: Ord, I: Iterator<Item = T>> {
    streams: Vec<I>,
    /// Max-heap on (value, stream index) - the plain `BinaryHeap` order,
    /// which is what a descending merge wants.
    heap: BinaryHeap<(T, usize)>,
    lower_bound: Option<T>,
}

impl<T: Ord, I: Iterator<Item = T>> ReverseMergeIterator<T, I> {
    pub fn new<S: IntoIterator<Item = I>>(streams: S) -> Self {
        let mut streams: Vec<I> = streams.into_iter().collect();
        let mut heap = BinaryHeap::with_capacity(streams.len());
        for (i, s) in streams.iter_mut().enumerate() {
            if let Some(v) = s.next() {
                heap.push((v, i));
            }
        }
        Self {
            streams,
            heap,
            lower_bound: None,
        }
    }

    /// Retreat past every entry strictly greater than `target`. After this
    /// call the next `next()` yields the largest value `<= target`, or the
    /// iterator is exhausted.
    pub fn seek_for_prev(&mut self, target: &T) {
        let mut new_heads: Vec<(T, usize)> = Vec::new();
        while let Some((value, _)) = self.heap.peek() {
            if value > target {
                let (_value, idx) = self.heap.pop().unwrap();
                let mut found: Option<T> = None;
                for v in self.streams[idx].by_ref() {
                    if &v <= target {
                        found = Some(v);
                        break;
                    }
                }
                if let Some(v) = found {
                    new_heads.push((v, idx));
                }
            } else {
                break;
            }
        }
        for head in new_heads {
            self.heap.push(head);
        }
    }

    /// Stop the descending scan at `bound`. The bound is inclusive: a value
    /// equal to it is the last one yielded.
    pub fn set_lower_bound(&mut self, bound: T) {
        self.lower_bound = Some(bound);
    }

    /// Drop the lower bound and let the scan run to the end of every source.
    pub fn clear_lower_bound(&mut self) {
        self.lower_bound = None;
    }

    /// The value the next `next()` will yield, without consuming it.
    pub fn peek(&self) -> Option<&T> {
        let head = self.heap.peek().map(|(value, _)| value)?;
        match &self.lower_bound {
            Some(lo) if head < lo => None,
            _ => Some(head),
        }
    }

    /// Streams still holding a head in the heap, ignoring the lower bound.
    pub fn live_streams(&self) -> usize {
        self.heap.len()
    }

    /// Streams the merge was constructed over, live or not.
    pub fn num_streams(&self) -> usize {
        self.streams.len()
    }
}

impl<T: Ord, I: Iterator<Item = T>> Iterator for ReverseMergeIterator<T, I> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.peek()?;
        let (value, idx) = self.heap.pop()?;
        if let Some(next_value) = self.streams[idx].next() {
            self.heap.push((next_value, idx));
        }
        Some(value)
    }
}

#[cfg(test)]
#[path = "reverse_tests.rs"]
mod tests;
