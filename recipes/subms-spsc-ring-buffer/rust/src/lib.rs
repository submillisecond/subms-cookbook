//! Wait-free SPSC ring buffer. Single producer, single consumer, bounded.
//!
//! Sized to a power of two; index modulo is a bitmask, not a `%`. Head and tail
//! counters live on their own cache lines (128-byte padding) to keep the
//! producer's writes from invalidating the consumer's read line. Each side
//! caches the opposite index and only re-reads through the atomic when its own
//! cache says full / empty.
//!
//! ```
//! use subms_spsc_ring_buffer::SpscRingBuffer;
//!
//! let (mut tx, mut rx) = SpscRingBuffer::with_capacity(64);
//! tx.try_push(42u32).unwrap();
//! assert_eq!(rx.try_pop(), Some(42));
//! assert_eq!(rx.try_pop(), None);
//! ```

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 128 bytes is the right padding for Apple Silicon + recent x86 (some
/// prefetchers operate on pairs of cache lines). Wastes ~96 bytes on 64-byte
/// line machines; the cost is negligible against the false-sharing tax.
#[repr(align(128))]
pub(crate) struct Padded(pub(crate) AtomicUsize);

pub(crate) struct Inner<T> {
    /// Next slot the consumer will read. Written by the consumer only.
    pub(crate) head: Padded,
    /// Next slot the producer will write. Written by the producer only.
    pub(crate) tail: Padded,
    /// Power-of-two slot count.
    pub(crate) capacity: usize,
    /// `capacity - 1`; lets `idx & mask` replace `idx % capacity`.
    pub(crate) mask: usize,
    /// Slot storage. Each cell is touched by exactly one of producer/consumer
    /// at any instant; `UnsafeCell` makes that explicit to the compiler.
    pub(crate) buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
}

// Producer/Consumer split is the type-level enforcement of SPSC. Inner itself
// is `Sync` because the two sides touch disjoint state.
unsafe impl<T: Send> Sync for Inner<T> {}
unsafe impl<T: Send> Send for Inner<T> {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        // Drain remaining entries so their Drop impls fire.
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Relaxed);
        for i in head..tail {
            unsafe {
                (*self.buf[i & self.mask].get()).assume_init_drop();
            }
        }
    }
}

/// Producer handle. Move it to the producing thread; the type system enforces
/// the SPSC invariant.
pub struct Producer<T> {
    pub(crate) inner: Arc<Inner<T>>,
    /// Last-known consumer head; re-read on under-run only.
    pub(crate) cached_head: usize,
}

unsafe impl<T: Send> Send for Producer<T> {}

/// Consumer handle. Move it to the consuming thread.
pub struct Consumer<T> {
    pub(crate) inner: Arc<Inner<T>>,
    /// Last-known producer tail; re-read on under-run only.
    pub(crate) cached_tail: usize,
}

unsafe impl<T: Send> Send for Consumer<T> {}

/// Constructor namespace. Use [`SpscRingBuffer::with_capacity`].
pub struct SpscRingBuffer;

impl SpscRingBuffer {
    /// Build a (producer, consumer) pair backed by a buffer of at least
    /// `requested_capacity` slots, rounded up to the next power of two.
    /// A floor of 2 is enforced.
    pub fn with_capacity<T>(requested_capacity: usize) -> (Producer<T>, Consumer<T>) {
        let cap = requested_capacity.max(2).next_power_of_two();
        let mut buf = Vec::with_capacity(cap);
        for _ in 0..cap {
            buf.push(UnsafeCell::new(MaybeUninit::<T>::uninit()));
        }
        let inner = Arc::new(Inner {
            head: Padded(AtomicUsize::new(0)),
            tail: Padded(AtomicUsize::new(0)),
            capacity: cap,
            mask: cap - 1,
            buf: buf.into_boxed_slice(),
        });
        (
            Producer {
                inner: inner.clone(),
                cached_head: 0,
            },
            Consumer {
                inner,
                cached_tail: 0,
            },
        )
    }
}

impl<T> Producer<T> {
    /// Push a value. Returns the input back as `Err(value)` if the buffer is
    /// full. Wait-free: at most one atomic load + one atomic store.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        let tail = self.inner.tail.0.load(Ordering::Relaxed);

        // First check against the cached head - no atomic traffic if it's stale-but-not-full.
        if tail.wrapping_sub(self.cached_head) == self.inner.capacity {
            // Cache says full; re-read the real consumer head.
            self.cached_head = self.inner.head.0.load(Ordering::Acquire);
            if tail.wrapping_sub(self.cached_head) == self.inner.capacity {
                return Err(value);
            }
        }

        // Slot is ours; write the value, then publish the new tail.
        unsafe {
            (*self.inner.buf[tail & self.inner.mask].get()).write(value);
        }
        self.inner
            .tail
            .0
            .store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Total slot count (power of two; not the requested capacity).
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }
}

impl<T> Consumer<T> {
    /// Pop a value. Returns `None` if the buffer is empty. Wait-free.
    pub fn try_pop(&mut self) -> Option<T> {
        let head = self.inner.head.0.load(Ordering::Relaxed);

        if head == self.cached_tail {
            self.cached_tail = self.inner.tail.0.load(Ordering::Acquire);
            if head == self.cached_tail {
                return None;
            }
        }

        let value = unsafe { (*self.inner.buf[head & self.inner.mask].get()).assume_init_read() };
        self.inner
            .head
            .0
            .store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// Total slot count (power of two).
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated on its own Cargo
// feature; the base ring stays wait-free SPSC + zero-dep std-only.
#[cfg(any(
    feature = "bulk",
    feature = "wait-strategies",
    feature = "mpsc-fan-in",
    feature = "mpmc-disruptor",
    feature = "metrics",
))]
pub mod features;

#[cfg(feature = "metrics")]
pub use features::metrics::{InstrumentedSpsc, RingMetrics, RingMetricsSnapshot};
#[cfg(feature = "mpmc-disruptor")]
pub use features::mpmc_disruptor::{DisruptorConsumer, DisruptorProducer, MpmcDisruptor};
#[cfg(feature = "mpsc-fan-in")]
pub use features::mpsc_fan_in::{MpscFanIn, MpscFanInConsumer, MpscFanInProducer};
#[cfg(feature = "wait-strategies")]
pub use features::wait_strategies::{
    BlockingSpscConsumer, BlockingSpscProducer, BusySpin, ParkStrategy, WaitStrategy, YieldStrategy,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
