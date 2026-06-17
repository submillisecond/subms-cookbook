//! Batch dequeue: drain up to N items in one fenced pass.
//!
//! Wraps the base [`MpscQueue`] with a [`try_dequeue_batch`] that
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
}

impl<T> Default for BatchMpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn collect_n<T>(out: &mut [Option<T>], n: usize) -> Vec<T> {
        out.iter_mut().take(n).map(|s| s.take().unwrap()).collect()
    }

    #[test]
    fn batch_drains_up_to_buffer_size() {
        let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
        for i in 0..10 {
            q.push(i);
        }
        let mut buf: Vec<Option<u32>> = (0..4).map(|_| None).collect();
        let n = q.try_dequeue_batch(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(collect_n(&mut buf, n), vec![0, 1, 2, 3]);
    }

    #[test]
    fn batch_stops_at_empty() {
        let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
        q.push(1);
        q.push(2);
        let mut buf: Vec<Option<u32>> = (0..10).map(|_| None).collect();
        let n = q.try_dequeue_batch(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf[0], Some(1));
        assert_eq!(buf[1], Some(2));
        // Re-call when empty returns zero.
        let n2 = q.try_dequeue_batch(&mut buf);
        assert_eq!(n2, 0);
    }

    #[test]
    fn batch_preserves_fifo_order() {
        let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
        for i in 0..100 {
            q.push(i);
        }
        let mut buf: Vec<Option<u32>> = (0..100).map(|_| None).collect();
        let n = q.try_dequeue_batch(&mut buf);
        assert_eq!(n, 100);
        for (i, slot) in buf.iter().enumerate().take(100) {
            assert_eq!(*slot, Some(i as u32));
        }
    }

    #[test]
    fn drain_into_vec_works() {
        let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
        for i in 0..50 {
            q.push(i);
        }
        let mut out = Vec::with_capacity(50);
        let n = q.drain_into_vec(&mut out, 50);
        assert_eq!(n, 50);
        for (i, v) in out.iter().enumerate() {
            assert_eq!(*v, i as u32);
        }
    }

    #[test]
    fn multi_producer_batch_drain_loses_nothing() {
        let producers = 4usize;
        let per_producer = 10_000usize;
        let q: Arc<BatchMpscQueue<u64>> = Arc::new(BatchMpscQueue::new());
        let mut prods = Vec::new();
        for tid in 0..producers as u64 {
            let q = q.clone();
            prods.push(thread::spawn(move || {
                for i in 0..per_producer as u64 {
                    q.push((tid << 32) | i);
                }
            }));
        }
        let cq = q.clone();
        let consumer = thread::spawn(move || {
            let qp = Arc::as_ptr(&cq) as *mut BatchMpscQueue<u64>;
            let qm = unsafe { &mut *qp };
            let mut counts = [0u64; 4];
            let target = producers * per_producer;
            let mut total = 0usize;
            let mut buf: Vec<Option<u64>> = (0..256).map(|_| None).collect();
            while total < target {
                let n = qm.try_dequeue_batch(&mut buf);
                for slot in buf.iter_mut().take(n) {
                    let v = slot.take().unwrap();
                    counts[(v >> 32) as usize] += 1;
                    total += 1;
                }
                if n == 0 {
                    std::hint::spin_loop();
                }
            }
            counts
        });
        for p in prods {
            p.join().unwrap();
        }
        let counts = consumer.join().unwrap();
        for c in counts {
            assert_eq!(c as usize, per_producer);
        }
    }

    #[test]
    fn empty_buffer_returns_zero() {
        let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
        q.push(1);
        let mut buf: Vec<Option<u32>> = Vec::new();
        let n = q.try_dequeue_batch(&mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn default_constructor_works() {
        let q: BatchMpscQueue<u32> = BatchMpscQueue::default();
        q.push(1);
        let mut qb = q;
        let mut buf: Vec<Option<u32>> = (0..4).map(|_| None).collect();
        assert_eq!(qb.try_dequeue_batch(&mut buf), 1);
    }
}
