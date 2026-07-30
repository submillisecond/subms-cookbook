//! Bulk transfer extensions on the base SPSC ring.
//!
//! `try_enqueue_bulk(&[T])` and `try_dequeue_bulk(&mut [T])` amortise the
//! cost of the per-item atomic load + cache check by issuing exactly one
//! acquire-load (only when the cached opposite-index is exhausted) and
//! exactly one release-store per call. Items are still committed in the
//! same order; consumers observing the new tail see all items that
//! preceded it.
//!
//! Returns the number of items actually transferred. A return < `slice.len()`
//! means the ring couldn't take the rest right now (full / empty); the
//! caller decides whether to back off.

use std::sync::atomic::Ordering;

use crate::{Consumer, Producer};

impl<T: Copy> Producer<T> {
    /// Copy as many items from `values` into the ring as will fit right now.
    /// Returns the count transferred. Single Release on the tail at the end.
    pub fn try_enqueue_bulk(&mut self, values: &[T]) -> usize {
        if values.is_empty() {
            return 0;
        }
        let tail = self.inner.tail.0.load(Ordering::Relaxed);
        let cap = self.inner.capacity;

        let mut free = cap - tail.wrapping_sub(self.cached_head);
        if free < values.len() {
            // Cache says we can't take them all; re-read the real head once
            // and recompute. One Acquire, not one per item.
            self.cached_head = self.inner.head.0.load(Ordering::Acquire);
            free = cap - tail.wrapping_sub(self.cached_head);
        }
        let n = free.min(values.len());
        if n == 0 {
            return 0;
        }
        for (i, v) in values.iter().take(n).enumerate() {
            unsafe {
                (*self.inner.buf[(tail.wrapping_add(i)) & self.inner.mask].get()).write(*v);
            }
        }
        // Single Release publishes all `n` slots at once.
        self.inner
            .tail
            .0
            .store(tail.wrapping_add(n), Ordering::Release);
        n
    }
}

impl<T: Copy> Consumer<T> {
    /// Drain up to `out.len()` items into `out`. Returns the count drained.
    /// Single Release on the head at the end.
    pub fn try_dequeue_bulk(&mut self, out: &mut [T]) -> usize {
        if out.is_empty() {
            return 0;
        }
        let head = self.inner.head.0.load(Ordering::Relaxed);

        let mut avail = self.cached_tail.wrapping_sub(head);
        if avail < out.len() {
            self.cached_tail = self.inner.tail.0.load(Ordering::Acquire);
            avail = self.cached_tail.wrapping_sub(head);
        }
        let n = avail.min(out.len());
        if n == 0 {
            return 0;
        }
        for (i, slot) in out.iter_mut().take(n).enumerate() {
            *slot = unsafe {
                (*self.inner.buf[(head.wrapping_add(i)) & self.inner.mask].get()).assume_init_read()
            };
        }
        self.inner
            .head
            .0
            .store(head.wrapping_add(n), Ordering::Release);
        n
    }
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
