//! Vyukov-style multi-producer single-consumer linked queue.
//!
//! Producers enqueue with one `swap` on the tail; the consumer drains by
//! following `next` pointers from the head. The dangling-tail window is the
//! load-bearing detail: between the producer's CAS-of-tail and the
//! prev.next = new link, the consumer can see `next == null` while there is
//! actually a publisher in flight. [`MpscQueue::try_pop`] returns
//! [`PopResult::Inconsistent`] in that window so the caller can spin or back
//! off rather than treating it as empty.
//!
//! ```
//! use subms_mpsc_queue::{MpscQueue, PopResult};
//! let mut q: MpscQueue<u32> = MpscQueue::new();
//! q.push(7);
//! q.push(8);
//! assert!(matches!(q.try_pop(), PopResult::Some(7)));
//! assert!(matches!(q.try_pop(), PopResult::Some(8)));
//! ```

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

struct Node<T> {
    value: UnsafeCell<Option<T>>,
    next: AtomicPtr<Node<T>>,
}

/// One-shot result of [`MpscQueue::try_pop`].
pub enum PopResult<T> {
    /// A value was dequeued.
    Some(T),
    /// Queue is truly empty.
    Empty,
    /// A producer is mid-publish; head was reached but `next` is not yet
    /// linked. Callers should retry (spin or back off).
    Inconsistent,
}

/// Multi-producer single-consumer linked queue.
///
/// Cloneable handles are not provided; share via `Arc<MpscQueue<T>>`.
/// Consumer methods (`try_pop`) require `&mut self` to encode single-consumer
/// invariant at the type level.
pub struct MpscQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: UnsafeCell<*mut Node<T>>,
    stub: Box<Node<T>>,
}

unsafe impl<T: Send> Sync for MpscQueue<T> {}
unsafe impl<T: Send> Send for MpscQueue<T> {}

impl<T> MpscQueue<T> {
    pub fn new() -> Self {
        let stub = Box::new(Node {
            value: UnsafeCell::new(None),
            next: AtomicPtr::new(ptr::null_mut()),
        });
        let stub_ptr = stub.as_ref() as *const Node<T> as *mut Node<T>;
        Self {
            head: AtomicPtr::new(stub_ptr),
            tail: UnsafeCell::new(stub_ptr),
            stub,
        }
    }

    /// Multi-producer push. Wait-free for the producer once the node is
    /// allocated.
    pub fn push(&self, value: T) {
        let node = Box::into_raw(Box::new(Node {
            value: UnsafeCell::new(Some(value)),
            next: AtomicPtr::new(ptr::null_mut()),
        }));
        // swap-head exchanges the publication point atomically. Producers
        // can race here; each gets a distinct prev.
        let prev = self.head.swap(node, Ordering::AcqRel);
        // The dangling-tail window opens here: prev exists, but prev.next is
        // still null until the next line. Consumer must tolerate it.
        unsafe { (*prev).next.store(node, Ordering::Release) };
    }

    /// Consume one entry. Returns [`PopResult::Inconsistent`] if a producer
    /// is mid-publish; callers should retry.
    ///
    /// Single consumer only: requires `&mut self`.
    pub fn try_pop(&mut self) -> PopResult<T> {
        // Safety: `tail` is consumer-private (only one consumer at a time).
        let tail = unsafe { *self.tail.get() };
        let next = unsafe { (*tail).next.load(Ordering::Acquire) };

        // The stub trick: tail starts at the stub. Once we've drained past
        // it, swap stub to the new tail so the head's reference is preserved.
        let stub_ptr = self.stub.as_ref() as *const Node<T> as *mut Node<T>;
        if tail == stub_ptr {
            if next.is_null() {
                // Stub is still the only node: either empty or producer
                // mid-publish.
                if self.head.load(Ordering::Acquire) == stub_ptr {
                    return PopResult::Empty;
                }
                return PopResult::Inconsistent;
            }
            // Move past the stub.
            unsafe { *self.tail.get() = next };
            let value = unsafe { (*next).value.get().replace(None) };
            return match value {
                Some(v) => PopResult::Some(v),
                None => PopResult::Inconsistent,
            };
        }

        if !next.is_null() {
            unsafe { *self.tail.get() = next };
            // Drop the consumed node now that tail has advanced past it.
            let consumed = unsafe { Box::from_raw(tail) };
            drop(consumed);
            let value = unsafe { (*next).value.get().replace(None) };
            return match value {
                Some(v) => PopResult::Some(v),
                None => PopResult::Inconsistent,
            };
        }

        // tail.next is null but tail is not the stub: either truly drained
        // or a producer is racing the link write.
        if self.head.load(Ordering::Acquire) == tail {
            PopResult::Empty
        } else {
            PopResult::Inconsistent
        }
    }
}

impl<T> Default for MpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for MpscQueue<T> {
    fn drop(&mut self) {
        // Drain remaining nodes so their values' Drop impls run.
        while let PopResult::Some(_) = self.try_pop() {}
        // The stub is owned by the Box field; nothing to do for it. Any
        // non-stub nodes were freed by try_pop as it walked past them.
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is independent of the base queue and
// gated by its own Cargo feature; `cargo add subms-mpsc-queue` alone
// keeps the base zero-dep + std-only shape.
//
// See README and the cookbook page for the per-feature p99 numbers
// and composition guidance.
#[cfg(any(
    feature = "mpmc",
    feature = "bounded",
    feature = "batch",
    feature = "metrics",
    feature = "affinity",
))]
pub mod features;

#[cfg(feature = "affinity")]
pub use features::affinity::{AffinityError, set_affinity};
#[cfg(feature = "batch")]
pub use features::batch::BatchMpscQueue;
#[cfg(feature = "bounded")]
pub use features::bounded::BoundedMpscQueue;
#[cfg(feature = "metrics")]
pub use features::metrics::{MetricsMpscQueue, QueueMetricsSnapshot};
#[cfg(feature = "mpmc")]
pub use features::mpmc::MpmcQueue;
