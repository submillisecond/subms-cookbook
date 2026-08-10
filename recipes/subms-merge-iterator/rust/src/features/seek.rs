//! Seekable k-way merge. Adds `seek(target)` that advances the iterator
//! past every entry strictly less than `target`. After `seek(t)` the
//! next `next()` returns the smallest value >= `t` (or `None` if no
//! source has any).
//!
//! Cost is bounded by the number of entries skipped plus a heap
//! reposition per advanced source. Repeated calls to `seek()` with a
//! monotonically non-decreasing target are cheap because each source's
//! head only moves forward.
//!
//! `set_upper_bound(hi)` closes the other end of a range scan: the
//! iterator reports exhausted once its head reaches `hi`, so
//! `seek(&lo)` + `set_upper_bound(&hi)` walks `[lo, hi)` and stops on
//! its own. The bound is EXCLUSIVE, matching RocksDB's
//! `iterate_upper_bound`.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct SeekableMergeIterator<T: Ord, I: Iterator<Item = T>> {
    streams: Vec<I>,
    heap: BinaryHeap<Reverse<(T, usize)>>,
    upper_bound: Option<T>,
}

impl<T: Ord, I: Iterator<Item = T>> SeekableMergeIterator<T, I> {
    pub fn new<S: IntoIterator<Item = I>>(streams: S) -> Self {
        let mut streams: Vec<I> = streams.into_iter().collect();
        let mut heap = BinaryHeap::with_capacity(streams.len());
        for (i, s) in streams.iter_mut().enumerate() {
            if let Some(v) = s.next() {
                heap.push(Reverse((v, i)));
            }
        }
        Self {
            streams,
            heap,
            upper_bound: None,
        }
    }

    /// Stop the scan before `bound`. The bound is exclusive: a value equal
    /// to it is not yielded. Setting a bound below the current head
    /// exhausts the iterator immediately.
    pub fn set_upper_bound(&mut self, bound: T) {
        self.upper_bound = Some(bound);
    }

    /// Drop the upper bound and let the scan run to the end of every source.
    pub fn clear_upper_bound(&mut self) {
        self.upper_bound = None;
    }

    /// The value the next `next()` will yield, without consuming it.
    /// Respects the upper bound, so a bounded scan peeks `None` at the
    /// same point it stops yielding.
    pub fn peek(&self) -> Option<&T> {
        let head = self.heap.peek().map(|Reverse((value, _))| value)?;
        match &self.upper_bound {
            Some(hi) if head >= hi => None,
            _ => Some(head),
        }
    }

    /// Streams still holding a head in the heap, ignoring the upper bound.
    pub fn live_streams(&self) -> usize {
        self.heap.len()
    }

    /// Streams the merge was constructed over, live or not.
    pub fn num_streams(&self) -> usize {
        self.streams.len()
    }

    /// Advance past every entry with key strictly less than `target`.
    /// After this call, the iterator's next yielded value is the
    /// smallest value >= `target` (or the iterator is exhausted).
    pub fn seek(&mut self, target: &T) {
        // Drain heads < target, advancing the underlying stream until
        // a head >= target appears (or the stream ends). Anything we
        // pull off then push back is the new minimum head for that
        // stream.
        let mut new_heads: Vec<(T, usize)> = Vec::new();
        while let Some(Reverse((value, idx))) = self.heap.peek() {
            if value < target {
                let Reverse((_value, idx)) = self.heap.pop().unwrap();
                // Walk this stream forward until we land on a value
                // >= target or the stream dries up.
                let mut found: Option<T> = None;
                for v in self.streams[idx].by_ref() {
                    if &v >= target {
                        found = Some(v);
                        break;
                    }
                }
                if let Some(v) = found {
                    new_heads.push((v, idx));
                }
            } else {
                // Heap head is already >= target; nothing more to do.
                let _ = idx;
                break;
            }
        }
        for (v, i) in new_heads {
            self.heap.push(Reverse((v, i)));
        }
    }
}

impl<T: Ord, I: Iterator<Item = T>> Iterator for SeekableMergeIterator<T, I> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.peek()?;
        let Reverse((value, idx)) = self.heap.pop()?;
        if let Some(next_value) = self.streams[idx].next() {
            self.heap.push(Reverse((next_value, idx)));
        }
        Some(value)
    }
}

#[cfg(test)]
#[path = "seek_tests.rs"]
mod tests;
