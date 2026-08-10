//! Multi-consumer extension: bounded MPMC ring with tail-sequence CAS.
//!
//! Disruptor-style barrier: per-slot sequence numbers gate both
//! producer claim (CAS the tail) and consumer claim (CAS the head).
//! Multiple consumers race; the loser sees a stale head and retries
//! with the new value. Optional [`MpmcQueue::cas_retries`] counts
//! contention for callers wiring it through the `metrics` feature.
//!
//! Both [`MpmcQueue::try_enqueue`] and [`MpmcQueue::try_dequeue`] are wait-free in the
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

    /// Monotonic count of slots ever claimed by producers.
    pub fn producer_index(&self) -> usize {
        self.tail.load(Ordering::Acquire)
    }

    /// Monotonic count of slots ever claimed by consumers.
    pub fn consumer_index(&self) -> usize {
        self.head.load(Ordering::Acquire)
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

    /// Drop everything currently readable and return the count. Any consumer
    /// may call it, and other consumers keep draining alongside, so the count
    /// is this caller's share rather than the queue's total.
    pub fn clear(&self) -> usize {
        let mut n = 0;
        while self.try_dequeue().is_some() {
            n += 1;
        }
        n
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

    /// Best-effort fullness. Stale the instant any consumer drains a slot.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }
}

impl<T> Drop for MpmcQueue<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
#[path = "mpmc_tests.rs"]
mod tests;
