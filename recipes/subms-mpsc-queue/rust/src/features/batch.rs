//! Batch dequeue: drain up to N items in one fenced pass.
//!
//! Wraps the base [`MpscQueue`] with a [`BatchMpscQueue::try_dequeue_batch`] that
//! pays one acquire-fence per call instead of one per item. The pass
//! follows `next` pointers from the consumer-private tail; the
//! single acquire on the head establishes the ordering boundary, and
//! every subsequent in-batch link read is relaxed because the chain
//! is already published.
//!
//! Stops early when:
//!   - `out` is full,
//!   - the chain ends (truly empty), or
//!   - a producer is mid-publish (dangling-tail window).
//!
//! Returns the number of items written to `out`.

use crate::{MpscQueue, PopResult};

/// Batch-draining wrapper around the base [`MpscQueue`].
pub struct BatchMpscQueue<T> {
    inner: MpscQueue<T>,
}

impl<T> BatchMpscQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: MpscQueue::new(),
        }
    }

    /// Same as the base [`MpscQueue::push`].
    pub fn push(&self, value: T) {
        self.inner.push(value);
    }

    /// Publish a whole run with one head swap. The producer-side mirror of
    /// [`Self::try_dequeue_batch`]: N items cost one atomic exchange rather
    /// than N. Returns the number published.
    pub fn push_batch<I: IntoIterator<Item = T>>(&self, values: I) -> usize {
        self.inner.push_batch(values)
    }

    /// Drain up to `out.len()` items into `out`. Returns the count.
    ///
    /// Stops early on dangling-tail or empty. Caller can spin / back
    /// off and re-call.
    pub fn try_dequeue_batch(&mut self, out: &mut [Option<T>]) -> usize {
        let mut n = 0;
        while n < out.len() {
            match self.inner.try_pop() {
                PopResult::Some(v) => {
                    out[n] = Some(v);
                    n += 1;
                }
                PopResult::Empty | PopResult::Inconsistent => break,
            }
        }
        n
    }

    /// Drain up to `limit` items straight into `f`, with no intermediate
    /// buffer. The callback form of JCTools' `drain(Consumer, limit)`, and the
    /// one to reach for when the consumer's work is per-item anyway.
    ///
    /// Stops early on empty or dangling-tail, exactly as
    /// [`Self::try_dequeue_batch`] does. Returns the count handed to `f`.
    pub fn drain<F: FnMut(T)>(&mut self, limit: usize, mut f: F) -> usize {
        let mut n = 0;
        while n < limit {
            match self.inner.try_pop() {
                PopResult::Some(v) => {
                    f(v);
                    n += 1;
                }
                PopResult::Empty | PopResult::Inconsistent => break,
            }
        }
        n
    }

    /// Convenience: drain into a `Vec`, returning the count drained.
    /// Pre-sizes the vec to `cap` before draining.
    pub fn drain_into_vec(&mut self, out: &mut Vec<T>, cap: usize) -> usize {
        let mut n = 0;
        while n < cap {
            match self.inner.try_pop() {
                PopResult::Some(v) => {
                    out.push(v);
                    n += 1;
                }
                PopResult::Empty | PopResult::Inconsistent => break,
            }
        }
        n
    }

    /// Borrow the next value without consuming it. See [`MpscQueue::peek`].
    pub fn peek(&mut self) -> Option<&T> {
        self.inner.peek()
    }

    /// See [`MpscQueue::is_empty`].
    pub fn is_empty(&mut self) -> bool {
        self.inner.is_empty()
    }

    /// See [`MpscQueue::len`]. O(n) in the backlog.
    pub fn len(&mut self) -> usize {
        self.inner.len()
    }

    /// See [`MpscQueue::clear`].
    pub fn clear(&mut self) -> usize {
        self.inner.clear()
    }
}

impl<T> Default for BatchMpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
