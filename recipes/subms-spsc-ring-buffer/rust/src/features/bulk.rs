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
mod tests {
    use std::thread;

    use crate::SpscRingBuffer;

    #[test]
    fn bulk_enqueue_then_bulk_dequeue_round_trip() {
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(16);
        let src: Vec<u32> = (0..10).collect();
        let n = tx.try_enqueue_bulk(&src);
        assert_eq!(n, 10);
        let mut out = [0u32; 10];
        let m = rx.try_dequeue_bulk(&mut out);
        assert_eq!(m, 10);
        assert_eq!(out, src.as_slice());
    }

    #[test]
    fn bulk_enqueue_partial_when_ring_almost_full() {
        let (mut tx, mut _rx) = SpscRingBuffer::with_capacity::<u32>(4);
        // Capacity 4: push 3 single items, then try to bulk-push 5 more.
        for i in 0..3u32 {
            tx.try_push(i).unwrap();
        }
        let src = [100u32, 101, 102, 103, 104];
        let n = tx.try_enqueue_bulk(&src);
        // Only 1 slot free.
        assert_eq!(n, 1);
    }

    #[test]
    fn bulk_dequeue_partial_when_ring_almost_empty() {
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(8);
        for i in 0..3u32 {
            tx.try_push(i).unwrap();
        }
        let mut out = [0u32; 10];
        let n = rx.try_dequeue_bulk(&mut out);
        assert_eq!(n, 3);
        assert_eq!(&out[..3], &[0u32, 1, 2]);
    }

    #[test]
    fn bulk_returns_zero_on_full_ring() {
        let (mut tx, _rx) = SpscRingBuffer::with_capacity::<u32>(4);
        for i in 0..4u32 {
            tx.try_push(i).unwrap();
        }
        let n = tx.try_enqueue_bulk(&[99u32, 100]);
        assert_eq!(n, 0);
    }

    #[test]
    fn bulk_returns_zero_on_empty_ring() {
        let (_tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let mut out = [0u32; 4];
        let n = rx.try_dequeue_bulk(&mut out);
        assert_eq!(n, 0);
    }

    #[test]
    fn bulk_handles_zero_length_slice() {
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
        assert_eq!(tx.try_enqueue_bulk(&[]), 0);
        let mut out: [u32; 0] = [];
        assert_eq!(rx.try_dequeue_bulk(&mut out), 0);
    }

    #[test]
    fn bulk_wraps_around_correctly() {
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(8);
        // Drain to start at index 6 - close to the wrap boundary.
        for i in 0..6u32 {
            tx.try_push(i).unwrap();
        }
        for _ in 0..6 {
            rx.try_pop();
        }
        // Now push 6 - which has to wrap from index 6 across 7 -> 0,1,2,3.
        let src: Vec<u32> = (10..16).collect();
        let n = tx.try_enqueue_bulk(&src);
        assert_eq!(n, 6);
        let mut out = [0u32; 6];
        let m = rx.try_dequeue_bulk(&mut out);
        assert_eq!(m, 6);
        assert_eq!(out, src.as_slice());
    }

    #[test]
    fn bulk_round_trip_under_two_threads() {
        let n_items = 200_000u32;
        let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(1024);

        let producer = thread::spawn(move || {
            let chunk: Vec<u32> = (0..32).collect();
            let mut sent = 0u32;
            while sent < n_items {
                let remaining = n_items - sent;
                let take = chunk.len().min(remaining as usize);
                // Build a chunk whose values are sent..sent+take.
                let buf: Vec<u32> = (sent..sent + take as u32).collect();
                let pushed = tx.try_enqueue_bulk(&buf) as u32;
                sent += pushed;
            }
        });

        let consumer = thread::spawn(move || {
            let mut next = 0u32;
            let mut buf = [0u32; 32];
            while next < n_items {
                let m = rx.try_dequeue_bulk(&mut buf);
                for &v in &buf[..m] {
                    assert_eq!(v, next, "out of order at {next}");
                    next += 1;
                }
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    }
}
