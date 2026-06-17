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
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use super::*;

    #[test]
    fn single_producer_acts_like_plain_spsc() {
        let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(1, 4);
        let p = &mut producers[0];
        p.try_push(7).unwrap();
        assert_eq!(consumer.try_pop(), Some(7));
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn round_robin_visits_every_producer() {
        let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(3, 4);
        producers[0].try_push(10).unwrap();
        producers[1].try_push(20).unwrap();
        producers[2].try_push(30).unwrap();
        // First pop: cursor starts at 0 -> ring 0 has 10.
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(consumer.try_pop().unwrap());
        }
        got.sort();
        assert_eq!(got, vec![10u32, 20, 30]);
    }

    #[test]
    fn quiet_producer_doesnt_block_busy_one() {
        // Producer 0 quiet, producer 1 busy. Consumer should still drain 1.
        let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(2, 16);
        for i in 0..10u32 {
            producers[1].try_push(i).unwrap();
        }
        let mut got = Vec::new();
        while let Some(v) = consumer.try_pop() {
            got.push(v);
        }
        assert_eq!(got, (0..10u32).collect::<Vec<_>>());
    }

    #[test]
    fn try_pop_on_all_empty_returns_none() {
        let (_producers, mut consumer) = MpscFanIn::with_capacity::<u32>(4, 4);
        assert_eq!(consumer.try_pop(), None);
    }

    #[test]
    fn three_producers_one_consumer_under_threads() {
        let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u64>(3, 256);
        let per_producer = 50_000u64;
        let consumed = Arc::new(AtomicUsize::new(0));
        let consumed_c = consumed.clone();

        let total = per_producer * 3;

        let consumer_t = thread::spawn(move || {
            let mut local = 0u64;
            while local < total {
                if let Some(_v) = consumer.try_pop() {
                    local += 1;
                    consumed_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        let mut handles = Vec::new();
        for (i, _) in (0..3).enumerate() {
            // Move the i-th producer into the spawned thread.
            let mut producer = producers.remove(0);
            handles.push(thread::spawn(move || {
                for j in 0..per_producer {
                    let v = (i as u64) * 1_000_000 + j;
                    while producer.try_push(v).is_err() {
                        std::hint::spin_loop();
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        consumer_t.join().unwrap();
        assert_eq!(consumed.load(Ordering::Relaxed), total as usize);
    }

    #[test]
    fn cursor_advances_past_drained_ring() {
        let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(2, 4);
        producers[0].try_push(1).unwrap();
        // pop from ring 0 -> cursor advances to 1.
        assert_eq!(consumer.try_pop(), Some(1));
        producers[1].try_push(2).unwrap();
        // The next pop starts at ring 1; should find 2.
        assert_eq!(consumer.try_pop(), Some(2));
    }
}
