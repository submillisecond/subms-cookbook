//! N-producer single-consumer fan-in over N independent SPSC rings.
//!
//! Each producer pushes into its own SPSC ring (so it stays wait-free against
//! its own counter), and the consumer round-robins across all rings. The
//! consumer never blocks any producer; producers never contend with each
//! other.
//!
//! Memory cost: `N * capacity` slots. Round-robin is fair under steady-state
//! load; under skewed load the consumer spends extra `try_pop` calls on
//! quiet producers but never starves a busy one.

use crate::{Consumer, Producer, SpscRingBuffer};

/// Builder + handle factory for an N-producer fan-in. After construction,
/// move each `MpscFanInProducer` to its producing thread and the single
/// `MpscFanInConsumer` to its consuming thread.
pub struct MpscFanIn;

impl MpscFanIn {
    /// Build `producer_count` SPSC rings of capacity `per_ring_capacity` and
    /// return matched producer / consumer handles.
    pub fn with_capacity<T: Send + 'static>(
        producer_count: usize,
        per_ring_capacity: usize,
    ) -> (Vec<MpscFanInProducer<T>>, MpscFanInConsumer<T>) {
        assert!(producer_count >= 1, "need at least one producer");
        let mut producers = Vec::with_capacity(producer_count);
        let mut consumers = Vec::with_capacity(producer_count);
        for _ in 0..producer_count {
            let (p, c) = SpscRingBuffer::with_capacity::<T>(per_ring_capacity);
            producers.push(MpscFanInProducer { inner: p });
            consumers.push(c);
        }
        (
            producers,
            MpscFanInConsumer {
                rings: consumers,
                cursor: 0,
            },
        )
    }
}

/// One producer side of an `MpscFanIn`. Wait-free against its own ring;
/// independent of any other producer.
pub struct MpscFanInProducer<T> {
    inner: Producer<T>,
}

impl<T> MpscFanInProducer<T> {
    /// Push a value into this producer's ring. Returns `Err(value)` if full.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        self.inner.try_push(value)
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

/// The single consumer side. Round-robins across the producer rings.
pub struct MpscFanInConsumer<T> {
    rings: Vec<Consumer<T>>,
    /// Next ring to probe; advances every successful pop so producers don't
    /// starve. Wraps modulo `rings.len()`.
    cursor: usize,
}

impl<T> MpscFanInConsumer<T> {
    /// Try one full round of probes across all producer rings. Returns the
    /// first value found, advancing the cursor past the producing ring so
    /// the next call starts at a fresh point.
    pub fn try_pop(&mut self) -> Option<T> {
        let n = self.rings.len();
        for offset in 0..n {
            let idx = (self.cursor + offset) % n;
            if let Some(v) = self.rings[idx].try_pop() {
                self.cursor = (idx + 1) % n;
                return Some(v);
            }
        }
        None
    }

    /// Producer count.
    pub fn producer_count(&self) -> usize {
        self.rings.len()
    }
}

#[cfg(test)]
#[path = "mpsc_fan_in_tests.rs"]
mod tests;
