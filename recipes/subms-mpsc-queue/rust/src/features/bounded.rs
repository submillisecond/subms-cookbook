//! Bounded MPSC queue: fixed-capacity ring buffer with backpressure.
//!
//! Producers see backpressure via [`BoundedMpscQueue::try_enqueue`]
//! returning the rejected value when the ring is full. Single consumer
//! only ([`BoundedMpscQueue::try_dequeue`] takes `&mut self`).
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
    /// Written only by the consumer. Atomic rather than a plain cell so the
    /// introspection getters stay sound when a producer thread calls them
    /// through a shared handle.
    head: AtomicUsize,
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
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Capacity (power-of-two; possibly larger than requested).
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Monotonic count of slots ever claimed by producers. Safe to read from
    /// any thread; pair it with [`Self::consumer_index`] to get lag, or sample
    /// it twice to get throughput without disturbing either end.
    pub fn producer_index(&self) -> usize {
        self.tail.load(Ordering::Acquire)
    }

    /// Monotonic count of slots ever consumed. Safe to read from any thread.
    pub fn consumer_index(&self) -> usize {
        self.head.load(Ordering::Acquire)
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
        let head = self.head.load(Ordering::Relaxed);
        let slot = &self.slots[head & self.mask];
        let seq = slot.seq.load(Ordering::Acquire);
        let diff = seq.wrapping_sub(head.wrapping_add(1)) as isize;
        if diff == 0 {
            let value = unsafe { (*slot.value.get()).assume_init_read() };
            // Mark slot ready for the next producer pass.
            slot.seq
                .store(head.wrapping_add(self.mask + 1), Ordering::Release);
            self.head.store(head.wrapping_add(1), Ordering::Release);
            Some(value)
        } else {
            None
        }
    }

    /// Borrow the next value without consuming it. `None` when the ring is
    /// empty. Consumer-side only.
    pub fn peek(&mut self) -> Option<&T> {
        let head = self.head.load(Ordering::Relaxed);
        let slot = &self.slots[head & self.mask];
        let seq = slot.seq.load(Ordering::Acquire);
        if seq.wrapping_sub(head.wrapping_add(1)) as isize == 0 {
            Some(unsafe { (*slot.value.get()).assume_init_ref() })
        } else {
            None
        }
    }

    /// Drop everything currently readable and return the count. Producers keep
    /// publishing throughout, so the ring is not guaranteed empty on return.
    /// Consumer-side only.
    pub fn clear(&mut self) -> usize {
        let mut n = 0;
        while self.try_dequeue().is_some() {
            n += 1;
        }
        n
    }

    /// Best-effort length. Approximate under producer contention.
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Best-effort fullness. A `true` can go stale the instant the consumer
    /// drains a slot, so branch on [`Self::try_enqueue`] instead of this when
    /// the answer decides whether a push lands.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }
}

impl<T> Drop for BoundedMpscQueue<T> {
    fn drop(&mut self) {
        // Drain remaining initialized slots so their destructors run.
        self.clear();
    }
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;
