//! K-way merge iterator. Min-heap of stream heads; pop the minimum, advance
//! that stream, push its next value if any.
//!
//! Streams must be sorted ascending. Output is the global sorted union.
//!
//! ```
//! use subms_merge_iterator::MergeIterator;
//! let streams: Vec<Box<dyn Iterator<Item = i32>>> = vec![
//!     Box::new(vec![1, 4, 7].into_iter()),
//!     Box::new(vec![2, 5, 8].into_iter()),
//!     Box::new(vec![3, 6, 9].into_iter()),
//! ];
//! let merged: Vec<_> = MergeIterator::new(streams).collect();
//! assert_eq!(merged, (1..=9).collect::<Vec<_>>());
//! ```

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct MergeIterator<T: Ord, I: Iterator<Item = T>> {
    streams: Vec<I>,
    /// Min-heap on (value, stream index). `Reverse` flips to min ordering.
    heap: BinaryHeap<Reverse<(T, usize)>>,
}

impl<T: Ord, I: Iterator<Item = T>> MergeIterator<T, I> {
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
}

impl<T: Ord, I: Iterator<Item = T>> Iterator for MergeIterator<T, I> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        let Reverse((value, idx)) = self.heap.pop()?;
        if let Some(next_value) = self.streams[idx].next() {
            self.heap.push(Reverse((next_value, idx)));
        }
        Some(value)
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is independent of the base merge iterator
// and gated by its own Cargo feature; the bare `cargo add
// subms-merge-iterator` shape stays zero-dep + std-only.
#[cfg(any(
    feature = "seek-to",
    feature = "tombstones",
    feature = "dedup",
    feature = "priority"
))]
pub mod features;

#[cfg(feature = "dedup")]
pub use features::dedup::{DedupEntry, DedupMergeIterator};
#[cfg(feature = "priority")]
pub use features::priority::{PriorityEntry, PriorityMergeIterator, PrioritySource};
#[cfg(feature = "seek-to")]
pub use features::seek::SeekableMergeIterator;
#[cfg(feature = "tombstones")]
pub use features::tombstones::{TombstoneEntry, TombstoneMergeIterator};
