//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the feed-handler handoff drops on a full ring, preserves order when drained,
//! and each optional feature holds its own contract. Std-only, feature-gated
//! the same way as the example.

use std::thread;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tick {
    seq: u64,
    price_cents: u32,
}

#[test]
fn base_drops_on_full_and_preserves_order_when_drained() {
    let (mut tx, mut small_rx) = SpscRingBuffer::with_capacity::<Tick>(4);
    let mut dropped = 0usize;
    for seq in 0..6u64 {
        if tx
            .try_push(Tick {
                seq,
                price_cents: 10_000,
            })
            .is_err()
        {
            dropped += 1;
        }
    }
    assert_eq!(
        dropped, 2,
        "two ticks past capacity are dropped, not blocked"
    );
    assert!(tx.is_full());
    assert_eq!(small_rx.len(), 4);
    assert_eq!(small_rx.peek().map(|t| t.seq), Some(0));
    assert_eq!(small_rx.clear(), 4);

    let n = 20_000u64;
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<Tick>(1024);
    let feed = thread::spawn(move || {
        for seq in 0..n {
            while tx
                .try_push(Tick {
                    seq,
                    price_cents: 10_000,
                })
                .is_err()
            {
                std::hint::spin_loop();
            }
        }
    });
    let strategy = thread::spawn(move || {
        let mut expected = 0u64;
        while expected < n {
            if let Some(tick) = rx.try_pop() {
                assert_eq!(tick.seq, expected, "ticks arrive in feed order");
                expected += 1;
            }
        }
        expected
    });
    feed.join().unwrap();
    assert_eq!(strategy.join().unwrap(), n);
}

#[cfg(feature = "bulk")]
#[test]
fn bulk_batch_round_trips_in_order() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<Tick>(16);
    let batch: Vec<Tick> = (0..10)
        .map(|seq| Tick {
            seq,
            price_cents: 20_000 + seq as u32,
        })
        .collect();
    assert_eq!(tx.try_enqueue_bulk(&batch), 10);

    let mut out = [Tick {
        seq: 0,
        price_cents: 0,
    }; 10];
    assert_eq!(rx.try_dequeue_bulk(&mut out), 10);
    assert_eq!(out.as_slice(), batch.as_slice());
}

#[cfg(feature = "wait-strategies")]
#[test]
fn blocking_handoff_preserves_order() {
    use super::{BlockingSpscConsumer, BlockingSpscProducer, YieldStrategy};

    let (tx, rx) = SpscRingBuffer::with_capacity::<Tick>(8);
    let mut producer = BlockingSpscProducer::new(tx, YieldStrategy);
    let mut consumer = BlockingSpscConsumer::new(rx, YieldStrategy);

    let n = 5_000u64;
    let feed = thread::spawn(move || {
        for seq in 0..n {
            producer.push(Tick {
                seq,
                price_cents: 30_000,
            });
        }
    });
    let strategy = thread::spawn(move || {
        for expected in 0..n {
            assert_eq!(consumer.pop().seq, expected);
        }
        n
    });
    feed.join().unwrap();
    assert_eq!(strategy.join().unwrap(), n);
}

#[cfg(feature = "mpsc-fan-in")]
#[test]
fn fan_in_drains_every_venue() {
    use super::MpscFanIn;

    let venues = 3usize;
    let per_venue = 10_000u64;
    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<Tick>(venues, 256);

    let mut feeds = Vec::new();
    for venue in 0..venues {
        let mut p = producers.remove(0);
        feeds.push(thread::spawn(move || {
            for seq in 0..per_venue {
                while p
                    .try_push(Tick {
                        seq,
                        price_cents: 40_000 + venue as u32,
                    })
                    .is_err()
                {
                    std::hint::spin_loop();
                }
            }
        }));
    }
    let total = per_venue * venues as u64;
    let strategy = thread::spawn(move || {
        let mut got = 0u64;
        while got < total {
            if consumer.try_pop().is_some() {
                got += 1;
            }
        }
        got
    });
    for f in feeds {
        f.join().unwrap();
    }
    assert_eq!(strategy.join().unwrap(), total);
}

#[cfg(feature = "mpmc-disruptor")]
#[test]
fn disruptor_broadcasts_every_tick_to_both_readers() {
    use super::MpmcDisruptor;

    let n = 8u64;
    let (producer, mut consumers) = MpmcDisruptor::with_consumers::<Tick>(16, 2);
    let (strategy, rest) = consumers.split_at_mut(1);
    let strategy = &mut strategy[0];
    let risk = &mut rest[0];

    let mut published = 0u64;
    let mut strat_seen = Vec::new();
    let mut risk_seen = Vec::new();
    while published < n {
        while published < n
            && producer
                .try_publish(Tick {
                    seq: published,
                    price_cents: 50_000,
                })
                .is_ok()
        {
            published += 1;
        }
        while let Some(t) = strategy.try_consume() {
            strat_seen.push(t.seq);
        }
        while let Some(t) = risk.try_consume() {
            risk_seen.push(t.seq);
        }
    }
    let expected: Vec<u64> = (0..n).collect();
    assert_eq!(strat_seen, expected);
    assert_eq!(risk_seen, expected);
}

#[cfg(feature = "metrics")]
#[test]
fn metrics_count_success_fail_and_peak_depth() {
    use super::InstrumentedSpsc;

    let (tx, rx) = SpscRingBuffer::with_capacity::<Tick>(4);
    let (mut tx, mut rx, metrics) = InstrumentedSpsc::wrap(tx, rx);
    for seq in 0..4u64 {
        tx.try_push(Tick {
            seq,
            price_cents: 60_000,
        })
        .unwrap();
    }
    assert!(
        tx.try_push(Tick {
            seq: 99,
            price_cents: 0
        })
        .is_err()
    );
    for _ in 0..4 {
        rx.try_pop().unwrap();
    }
    assert!(rx.try_pop().is_none());

    let snap = metrics.snapshot();
    assert_eq!(snap.enqueue_success, 4);
    assert_eq!(snap.enqueue_fail, 1);
    assert_eq!(snap.dequeue_success, 4);
    assert_eq!(snap.dequeue_fail, 1);
    assert_eq!(snap.max_depth_observed, 4);
}
