//! Multi-producer multi-consumer ring with sequence-barrier gating.
//!
//! Based on the LMAX Disruptor pattern. Three sequences:
//!
//! - `producer_cursor`: next index a producer will try to claim. CAS'd up
//!   by `try_claim`.
//! - `published[idx]`: per-slot atomic published flag. A claimed slot is
//!   marked published once the producer finishes writing.
//! - `consumer_cursor[i]`: per-consumer next index to read. Owned by that
//!   consumer; advanced after a successful read.
//!
//! Producer gating: a producer cannot claim slot `s` until the slowest
//! consumer has passed `s - capacity`. Consumer gating: a consumer cannot
//! read slot `s` until `published[s & mask]` matches the round it's in.
//! `published` stores the absolute sequence value of the last publish so
//! producers and consumers can distinguish round N from round N+1 in the
//! same slot.
//!
//! Independent of the base SPSC structures - this is a separate ring with
//! different invariants.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

const UNPUBLISHED: i64 = -1;

struct DisruptorInner<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// `published[i]` holds the absolute sequence number of the last
    /// publish into slot `i`, or `UNPUBLISHED`.
    published: Box<[AtomicI64]>,
    /// Next sequence a producer will try to claim (incremented via CAS).
    producer_cursor: AtomicI64,
    /// One cursor per consumer. Producers gate behind the slowest.
    consumer_cursors: Box<[AtomicI64]>,
    capacity: i64,
    mask: i64,
}

unsafe impl<T: Send> Sync for DisruptorInner<T> {}
unsafe impl<T: Send> Send for DisruptorInner<T> {}

impl<T> Drop for DisruptorInner<T> {
    fn drop(&mut self) {
        // Drop any item that was published but not consumed.
        for slot in self.published.iter() {
            let seq = slot.load(Ordering::Relaxed);
            if seq != UNPUBLISHED {
                let idx = (seq & self.mask) as usize;
                // Only drop if no consumer has passed this sequence.
                let min_consumer = self
                    .consumer_cursors
                    .iter()
                    .map(|c| c.load(Ordering::Relaxed))
                    .min()
                    .unwrap_or(-1);
                if seq > min_consumer {
                    unsafe {
                        (*self.buf[idx].get()).assume_init_drop();
                    }
                }
            }
        }
    }
}

/// Builder + constructor. Use `MpmcDisruptor::with_consumers`.
pub struct MpmcDisruptor;

impl MpmcDisruptor {
    /// Build a disruptor with `consumer_count` consumers and at least
    /// `requested_capacity` slots (rounded up to power of two, floor 2).
    /// All consumers see every published item (broadcast); for work-stealing
    /// semantics use `MpscFanIn` with a single consumer per producer instead.
    pub fn with_consumers<T: Send + 'static>(
        requested_capacity: usize,
        consumer_count: usize,
    ) -> (DisruptorProducer<T>, Vec<DisruptorConsumer<T>>) {
        assert!(consumer_count >= 1, "need at least one consumer");
        let cap = requested_capacity.max(2).next_power_of_two();
        let mut buf = Vec::with_capacity(cap);
        let mut published = Vec::with_capacity(cap);
        for _ in 0..cap {
            buf.push(UnsafeCell::new(MaybeUninit::<T>::uninit()));
            published.push(AtomicI64::new(UNPUBLISHED));
        }
        let mut cursors = Vec::with_capacity(consumer_count);
        for _ in 0..consumer_count {
            cursors.push(AtomicI64::new(-1));
        }
        let inner = Arc::new(DisruptorInner {
            buf: buf.into_boxed_slice(),
            published: published.into_boxed_slice(),
            producer_cursor: AtomicI64::new(-1),
            consumer_cursors: cursors.into_boxed_slice(),
            capacity: cap as i64,
            mask: (cap - 1) as i64,
        });
        let producer = DisruptorProducer {
            inner: inner.clone(),
        };
        let consumers = (0..consumer_count)
            .map(|i| DisruptorConsumer {
                inner: inner.clone(),
                idx: i,
                next: 0,
            })
            .collect();
        (producer, consumers)
    }
}

/// Multi-producer side. Cheap to `Clone`; share across producer threads.
pub struct DisruptorProducer<T> {
    inner: Arc<DisruptorInner<T>>,
}

impl<T> Clone for DisruptorProducer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

unsafe impl<T: Send> Send for DisruptorProducer<T> {}
unsafe impl<T: Send> Sync for DisruptorProducer<T> {}

impl<T> DisruptorProducer<T> {
    /// Try to publish a value. Returns the input back on contention OR
    /// when the slowest consumer hasn't caught up yet.
    pub fn try_publish(&self, value: T) -> Result<(), T> {
        loop {
            let cur = self.inner.producer_cursor.load(Ordering::Relaxed);
            let next = cur + 1;

            // Is the slot we'd claim still in-flight for some consumer?
            // Find the slowest consumer and check.
            let mut min_consumer = i64::MAX;
            for c in self.inner.consumer_cursors.iter() {
                let v = c.load(Ordering::Acquire);
                if v < min_consumer {
                    min_consumer = v;
                }
            }
            if next - min_consumer > self.inner.capacity {
                // Ring full from the slowest consumer's perspective.
                return Err(value);
            }

            // Claim the slot via CAS.
            match self.inner.producer_cursor.compare_exchange_weak(
                cur,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We own slot `next`. Wait until the previous occupant
                    // of slot index (next & mask) was fully consumed - it
                    // either is UNPUBLISHED (first round) or carries a
                    // sequence number `next - capacity`, which means it's
                    // safe to overwrite ONLY if all consumers have read it.
                    // The min_consumer check above already guarantees this
                    // for the round; but other producers may have claimed
                    // higher slots after we read the cursors. Re-confirm
                    // by waiting for the slot's `published` to be cleared
                    // for the previous round.
                    let idx = (next & self.inner.mask) as usize;
                    let prev_round = next - self.inner.capacity;
                    if prev_round >= 0 {
                        // Spin until the slot is no longer holding the
                        // prior round (consumer cursors must have advanced
                        // past it). This is the rare slow-path; uncontested
                        // claims skip it entirely.
                        while self
                            .inner
                            .consumer_cursors
                            .iter()
                            .any(|c| c.load(Ordering::Acquire) < prev_round)
                        {
                            std::hint::spin_loop();
                        }
                    }

                    unsafe {
                        (*self.inner.buf[idx].get()).write(value);
                    }
                    // Publish: store our absolute sequence so consumers can
                    // distinguish round-0 from round-1 in the same slot.
                    self.inner.published[idx].store(next, Ordering::Release);
                    return Ok(());
                }
                Err(_) => {
                    // Contention; loop and retry.
                    std::hint::spin_loop();
                }
            }
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity as usize
    }
}

/// One consumer side. Each consumer sees every published item.
pub struct DisruptorConsumer<T> {
    inner: Arc<DisruptorInner<T>>,
    idx: usize,
    next: i64,
}

unsafe impl<T: Send> Send for DisruptorConsumer<T> {}

impl<T: Clone> DisruptorConsumer<T> {
    /// Returns a clone of the next published item, or `None` if not ready.
    /// `Clone` because each consumer of a broadcast disruptor sees the
    /// same item; we cannot move out of the slot.
    pub fn try_consume(&mut self) -> Option<T> {
        let seq = self.next;
        let idx = (seq & self.inner.mask) as usize;
        if self.inner.published[idx].load(Ordering::Acquire) != seq {
            return None;
        }
        // Item is published; clone it out and advance.
        let v = unsafe { (*self.inner.buf[idx].get()).assume_init_ref() }.clone();
        self.next = seq + 1;
        self.inner.consumer_cursors[self.idx].store(seq, Ordering::Release);
        Some(v)
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity as usize
    }
}

#[cfg(test)]
#[path = "mpmc_disruptor_tests.rs"]
mod tests;
