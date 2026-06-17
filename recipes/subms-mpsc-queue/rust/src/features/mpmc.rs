//! Multi-consumer extension: bounded MPMC ring with tail-sequence CAS.
//!
//! Disruptor-style barrier: per-slot sequence numbers gate both
//! producer claim (CAS the tail) and consumer claim (CAS the head).
//! Multiple consumers race; the loser sees a stale head and retries
//! with the new value. Optional [`MpmcQueue::cas_retries`] counts
//! contention for callers wiring it through the `metrics` feature.
//!
//! Both [`try_enqueue`] and [`try_dequeue`] are wait-free in the
//! uncontended case and bounded-retry under contention.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Bounded MPMC ring queue. Capacity is rounded up to the next power
/// of two (minimum 2).
pub struct MpmcQueue<T> {
    mask: usize,
    slots: Box<[Slot<T>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    cas_retries: AtomicU64,
}

struct Slot<T> {
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send> Sync for MpmcQueue<T> {}
unsafe impl<T: Send> Send for MpmcQueue<T> {}

impl<T> MpmcQueue<T> {
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
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            cas_retries: AtomicU64::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Total CAS retries (both producers losing tail-CAS and consumers
    /// losing head-CAS). Useful for diagnosing contention; ignored by
    /// the hot path otherwise.
    pub fn cas_retries(&self) -> u64 {
        self.cas_retries.load(Ordering::Relaxed)
    }

    /// Multi-producer enqueue. Returns `Err(value)` if the ring is
    /// full.
    pub fn try_enqueue(&self, value: T) -> Result<(), T> {
        let mut tail = self.tail.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[tail & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
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
                    Err(t) => {
                        self.cas_retries.fetch_add(1, Ordering::Relaxed);
                        tail = t;
                    }
                }
            } else if diff < 0 {
                return Err(value);
            } else {
                tail = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    /// Multi-consumer dequeue. Returns `None` if the ring is empty.
    pub fn try_dequeue(&self) -> Option<T> {
        let mut head = self.head.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[head & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq.wrapping_sub(head.wrapping_add(1)) as isize;
            if diff == 0 {
                match self.head.compare_exchange_weak(
                    head,
                    head.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let value = unsafe { (*slot.value.get()).assume_init_read() };
                        slot.seq
                            .store(head.wrapping_add(self.mask + 1), Ordering::Release);
                        return Some(value);
                    }
                    Err(h) => {
                        self.cas_retries.fetch_add(1, Ordering::Relaxed);
                        head = h;
                    }
                }
            } else if diff < 0 {
                return None;
            } else {
                head = self.head.load(Ordering::Relaxed);
            }
        }
    }

    /// Approximate length.
    pub fn len(&self) -> usize {
        let h = self.head.load(Ordering::Acquire);
        let t = self.tail.load(Ordering::Acquire);
        t.wrapping_sub(h)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Drop for MpmcQueue<T> {
    fn drop(&mut self) {
        while self.try_dequeue().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::thread;

    #[test]
    fn single_thread_enqueue_dequeue() {
        let q: MpmcQueue<u32> = MpmcQueue::new(4);
        assert!(q.try_enqueue(7).is_ok());
        assert_eq!(q.try_dequeue(), Some(7));
        assert_eq!(q.try_dequeue(), None);
    }

    #[test]
    fn full_ring_rejects_with_value() {
        let q: MpmcQueue<u32> = MpmcQueue::new(2);
        assert!(q.try_enqueue(1).is_ok());
        assert!(q.try_enqueue(2).is_ok());
        assert_eq!(q.try_enqueue(3), Err(3));
    }

    #[test]
    fn fifo_within_single_producer_single_consumer() {
        let q: MpmcQueue<u32> = MpmcQueue::new(64);
        for i in 0..64 {
            q.try_enqueue(i).unwrap();
        }
        for i in 0..64 {
            assert_eq!(q.try_dequeue(), Some(i));
        }
    }

    #[test]
    fn multi_consumer_drains_all_items_exactly_once() {
        let producers = 4usize;
        let consumers = 4usize;
        let per_producer = 2_500usize;
        let q: Arc<MpmcQueue<u64>> = Arc::new(MpmcQueue::new(1024));

        let mut prods = Vec::new();
        for tid in 0..producers as u64 {
            let q = q.clone();
            prods.push(thread::spawn(move || {
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

        let total = producers * per_producer;
        let total_consumed = Arc::new(AtomicUsize::new(0));
        let mut cons = Vec::new();
        for _ in 0..consumers {
            let q = q.clone();
            let total_consumed = total_consumed.clone();
            cons.push(thread::spawn(move || {
                let mut local = 0u64;
                loop {
                    if let Some(_v) = q.try_dequeue() {
                        local += 1;
                        total_consumed.fetch_add(1, Ordering::Relaxed);
                    } else if total_consumed.load(Ordering::Relaxed) >= total {
                        break;
                    } else {
                        std::hint::spin_loop();
                    }
                }
                local
            }));
        }

        for p in prods {
            p.join().unwrap();
        }
        let mut got = 0u64;
        for c in cons {
            got += c.join().unwrap();
        }
        assert_eq!(got as usize, total);
        assert_eq!(total_consumed.load(Ordering::Relaxed), total);
    }

    #[test]
    fn cas_retries_increments_under_contention() {
        let producers = 8usize;
        let per_producer = 5_000usize;
        let q: Arc<MpmcQueue<u32>> = Arc::new(MpmcQueue::new(2048));
        let mut prods = Vec::new();
        for _ in 0..producers {
            let q = q.clone();
            prods.push(thread::spawn(move || {
                let mut i = 0;
                while i < per_producer {
                    if q.try_enqueue(i as u32).is_ok() {
                        i += 1;
                    }
                }
            }));
        }
        let dq = q.clone();
        let consumer = thread::spawn(move || {
            let mut got = 0;
            let total = producers * per_producer;
            while got < total {
                if dq.try_dequeue().is_some() {
                    got += 1;
                } else {
                    std::hint::spin_loop();
                }
            }
        });
        for p in prods {
            p.join().unwrap();
        }
        consumer.join().unwrap();
        // With 8 producers hitting a hot tail, contention is essentially
        // guaranteed. Loose lower-bound to keep the test stable.
        assert!(q.cas_retries() > 0, "expected non-zero retries");
    }

    #[test]
    fn drop_runs_for_pending_items() {
        struct DropCounted(Arc<AtomicUsize>);
        impl Drop for DropCounted {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let q: MpmcQueue<DropCounted> = MpmcQueue::new(4);
            assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
            assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
        }
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn drain_then_refill_wraps_ring() {
        let q: MpmcQueue<u32> = MpmcQueue::new(4);
        for round in 0..10 {
            for i in 0..4 {
                q.try_enqueue(round * 4 + i).unwrap();
            }
            for i in 0..4 {
                assert_eq!(q.try_dequeue(), Some(round * 4 + i));
            }
        }
    }
}
