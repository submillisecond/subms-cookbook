//! Bounded MPSC queue: fixed-capacity ring buffer with backpressure.
//!
//! Producers see backpressure via [`BoundedMpscQueue::try_enqueue`]
//! returning the rejected value when the ring is full. Single consumer
//! only ([`try_dequeue`] takes `&mut self`).
//!
//! Layout: power-of-two capacity, per-slot sequence numbers. Producers
//! CAS the tail to claim a slot, then write the value and bump the
//! slot's sequence to publish. The consumer reads slots in order and
//! advances head once each is consumed.
//!
//! `try_enqueue` is wait-free in the uncontended case and bounded-retry
//! under contention (each retry corresponds to a competing producer
//! that won the CAS).

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bounded MPSC ring queue. Capacity is rounded up to the next power
/// of two (minimum 2) so the modulo can be a bitmask.
pub struct BoundedMpscQueue<T> {
    mask: usize,
    slots: Box<[Slot<T>]>,
    head: UnsafeCell<usize>,
    tail: AtomicUsize,
}

struct Slot<T> {
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send> Sync for BoundedMpscQueue<T> {}
unsafe impl<T: Send> Send for BoundedMpscQueue<T> {}

impl<T> BoundedMpscQueue<T> {
    /// New empty queue. `capacity` is rounded up to a power of two,
    /// minimum 2.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        let mut slots = Vec::with_capacity(cap);
        for i in 0..cap {
            slots.push(Slot {
                seq: AtomicUsize::new(i),
                value: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }
        Self {
            mask: cap - 1,
            slots: slots.into_boxed_slice(),
            head: UnsafeCell::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Capacity (power-of-two; possibly larger than requested).
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Multi-producer push. Returns `Err(value)` when the ring is
    /// full so the caller can retry, drop, or apply backpressure.
    pub fn try_enqueue(&self, value: T) -> Result<(), T> {
        let mut tail = self.tail.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[tail & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            // Slot is open for write when seq == tail. seq < tail means
            // a consumer hasn't caught up yet (full); seq > tail means
            // another producer already claimed this slot.
            let diff = seq.wrapping_sub(tail) as isize;
            if diff == 0 {
                match self.tail.compare_exchange_weak(
                    tail,
                    tail.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        unsafe { (*slot.value.get()).write(value) };
                        slot.seq.store(tail.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    Err(t) => tail = t,
                }
            } else if diff < 0 {
                // Queue is full.
                return Err(value);
            } else {
                // Another producer is ahead; refresh and retry.
                tail = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    /// Single-consumer pop. Returns `None` when the ring is empty.
    pub fn try_dequeue(&mut self) -> Option<T> {
        let head = unsafe { *self.head.get() };
        let slot = &self.slots[head & self.mask];
        let seq = slot.seq.load(Ordering::Acquire);
        let diff = seq.wrapping_sub(head.wrapping_add(1)) as isize;
        if diff == 0 {
            let value = unsafe { (*slot.value.get()).assume_init_read() };
            // Mark slot ready for the next producer pass.
            slot.seq
                .store(head.wrapping_add(self.mask + 1), Ordering::Release);
            unsafe { *self.head.get() = head.wrapping_add(1) };
            Some(value)
        } else {
            None
        }
    }

    /// Best-effort length. Approximate under producer contention.
    pub fn len(&self) -> usize {
        let head = unsafe { *self.head.get() };
        let tail = self.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Drop for BoundedMpscQueue<T> {
    fn drop(&mut self) {
        // Drain remaining initialized slots so their destructors run.
        while self.try_dequeue().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn enqueue_dequeue_single_item() {
        let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
        assert!(q.try_enqueue(42).is_ok());
        assert_eq!(q.try_dequeue(), Some(42));
        assert_eq!(q.try_dequeue(), None);
    }

    #[test]
    fn capacity_is_power_of_two() {
        let q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(5);
        assert_eq!(q.capacity(), 8);
        let q2: BoundedMpscQueue<u32> = BoundedMpscQueue::new(1);
        assert_eq!(q2.capacity(), 2);
    }

    #[test]
    fn enqueue_full_returns_value_back() {
        let q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
        for i in 0..4 {
            assert!(q.try_enqueue(i).is_ok());
        }
        // 4 cap, all slots in use.
        let rejected = q.try_enqueue(99);
        assert_eq!(rejected, Err(99));
    }

    #[test]
    fn fifo_order_single_producer() {
        let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(16);
        for i in 0..16 {
            assert!(q.try_enqueue(i).is_ok());
        }
        for i in 0..16 {
            assert_eq!(q.try_dequeue(), Some(i));
        }
    }

    #[test]
    fn drain_then_refill_wraps_ring() {
        let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
        for round in 0..10 {
            for i in 0..4 {
                assert!(q.try_enqueue(round * 4 + i).is_ok());
            }
            for i in 0..4 {
                assert_eq!(q.try_dequeue(), Some(round * 4 + i));
            }
        }
    }

    #[test]
    fn multi_producer_no_lost_items() {
        let producers = 4usize;
        let per_producer = 5_000usize;
        let q: Arc<BoundedMpscQueue<u64>> = Arc::new(BoundedMpscQueue::new(1024));
        let stop = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for tid in 0..producers as u64 {
            let q = q.clone();
            handles.push(thread::spawn(move || {
                let mut i = 0u64;
                while i < per_producer as u64 {
                    if q.try_enqueue((tid << 32) | i).is_ok() {
                        i += 1;
                    } else {
                        std::hint::spin_loop();
                    }
                }
            }));
        }
        let cq = q.clone();
        let cstop = stop.clone();
        let consumer = thread::spawn(move || {
            // Manual pointer trick to call try_dequeue (&mut self) on a
            // shared Arc; only one consumer thread exists.
            let qp = Arc::as_ptr(&cq) as *mut BoundedMpscQueue<u64>;
            let qm = unsafe { &mut *qp };
            let mut counts = [0u64; 4];
            let mut total = 0usize;
            while total < producers * per_producer {
                if let Some(v) = qm.try_dequeue() {
                    counts[(v >> 32) as usize] += 1;
                    total += 1;
                } else {
                    std::hint::spin_loop();
                }
            }
            cstop.store(1, Ordering::Relaxed);
            counts
        });
        for h in handles {
            h.join().unwrap();
        }
        let counts = consumer.join().unwrap();
        for c in counts {
            assert_eq!(c as usize, per_producer);
        }
    }

    #[test]
    fn drops_pending_items_on_destruction() {
        struct DropCounted(Arc<AtomicUsize>);
        impl Drop for DropCounted {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let q: BoundedMpscQueue<DropCounted> = BoundedMpscQueue::new(4);
            assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
            assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
            assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
        }
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn len_tracks_outstanding_items() {
        let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(8);
        assert!(q.is_empty());
        q.try_enqueue(1).unwrap();
        q.try_enqueue(2).unwrap();
        assert_eq!(q.len(), 2);
        q.try_dequeue().unwrap();
        assert_eq!(q.len(), 1);
    }
}
