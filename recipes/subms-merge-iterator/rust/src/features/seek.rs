//! Seekable k-way merge. Adds `seek(target)` that advances the iterator
//! past every entry strictly less than `target`. After `seek(t)` the
//! next `next()` returns the smallest value >= `t` (or `None` if no
//! source has any).
//!
//! Cost is bounded by the number of entries skipped plus a heap
//! reposition per advanced source. Repeated calls to `seek()` with a
//! monotonically non-decreasing target are cheap because each source's
//! head only moves forward.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct SeekableMergeIterator<T: Ord, I: Iterator<Item = T>> {
    streams: Vec<I>,
    heap: BinaryHeap<Reverse<(T, usize)>>,
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
        Self { streams, heap }
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
        let Reverse((value, idx)) = self.heap.pop()?;
        if let Some(next_value) = self.streams[idx].next() {
            self.heap.push(Reverse((next_value, idx)));
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streams<const N: usize>(data: [Vec<i32>; N]) -> Vec<std::vec::IntoIter<i32>> {
        data.into_iter().map(|v| v.into_iter()).collect()
    }

    #[test]
    fn seek_lands_on_smallest_ge_target() {
        let mut it = SeekableMergeIterator::new(streams([
            vec![1, 4, 7, 10],
            vec![2, 5, 8, 11],
            vec![3, 6, 9, 12],
        ]));
        it.seek(&6);
        let rest: Vec<_> = it.collect();
        assert_eq!(rest, vec![6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn seek_with_target_below_min_is_noop() {
        let mut it = SeekableMergeIterator::new(streams([vec![5, 6, 7], vec![8, 9, 10]]));
        it.seek(&0);
        let rest: Vec<_> = it.collect();
        assert_eq!(rest, vec![5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn seek_past_end_exhausts_iterator() {
        let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 3], vec![4, 5, 6]]));
        it.seek(&1000);
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn seek_on_empty_sources_is_safe() {
        let mut it = SeekableMergeIterator::new(streams([vec![], vec![], vec![]]));
        it.seek(&42);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn repeated_seeks_are_monotonic() {
        let mut it = SeekableMergeIterator::new(streams([
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            vec![15, 16, 17],
        ]));
        it.seek(&5);
        assert_eq!(it.next(), Some(5));
        it.seek(&8);
        assert_eq!(it.next(), Some(8));
        it.seek(&14);
        assert_eq!(it.next(), Some(15));
        it.seek(&20);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn seek_target_exactly_present_yields_it() {
        let mut it =
            SeekableMergeIterator::new(streams([vec![1, 5, 9], vec![2, 6, 10], vec![3, 7, 11]]));
        it.seek(&7);
        let rest: Vec<_> = it.collect();
        assert_eq!(rest, vec![7, 9, 10, 11]);
    }

    #[test]
    fn seek_with_some_exhausted_streams() {
        // Stream 0 ends well before seek target.
        let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 3], vec![10, 20, 30]]));
        it.seek(&15);
        let rest: Vec<_> = it.collect();
        assert_eq!(rest, vec![20, 30]);
    }
}
