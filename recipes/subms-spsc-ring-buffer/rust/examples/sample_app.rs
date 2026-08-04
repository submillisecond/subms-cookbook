//! Sample app: a tour of `subms-spsc-ring-buffer`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features bulk`) to see the feature
//! sections light up.
//!
//! Domain: a market-data feed handler thread hands ticks to a strategy thread
//! over the wait-free ring. A full ring makes the feed handler drop the tick
//! (backpressure) rather than block the hot path.
//!
//! * base            - feed-handler -> strategy handoff, drop-on-full
//! * bulk            - drain a NIC receive batch in one fenced call
//! * wait-strategies - a blocking handoff when a wakeup cost is affordable
//! * mpsc-fan-in     - many venue feeds into one strategy, still wait-free per feed
//! * mpmc-disruptor  - broadcast one tick stream to strategy + risk monitor
//! * metrics         - per-instance enqueue/dequeue + max-depth counters

use std::thread;

use subms_spsc_ring_buffer::SpscRingBuffer;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tick {
    seq: u64,
    price_cents: u32,
}

fn main() {
    base_feed_to_strategy();

    #[cfg(feature = "bulk")]
    bulk_batch_ingest();

    #[cfg(feature = "wait-strategies")]
    blocking_handoff();

    #[cfg(feature = "mpsc-fan-in")]
    many_venue_fan_in();

    #[cfg(feature = "mpmc-disruptor")]
    broadcast_to_strategy_and_risk();

    #[cfg(feature = "metrics")]
    instrumented_handoff();
}

/// Base API: a feed-handler thread pushes ticks to a strategy thread. Both ends
/// are wait-free; when the ring fills, `try_push` hands the tick back so the
/// feed handler drops it instead of stalling the hot path.
fn base_feed_to_strategy() {
    println!("== base: feed-handler -> strategy handoff ==");

    // Drop-on-full is the caller's decision. Shown deterministically on a
    // small ring: capacity 4, six ticks offered, the last two are dropped.
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<Tick>(4);
    let mut dropped = 0usize;
    for seq in 0..6u64 {
        let tick = Tick {
            seq,
            price_cents: 10_000 + seq as u32,
        };
        if tx.try_push(tick).is_err() {
            dropped += 1;
        }
    }
    println!("  cap-4 ring, 6 offered -> {dropped} dropped under backpressure");
    assert_eq!(
        dropped, 2,
        "two ticks past capacity are dropped, not blocked"
    );

    // Occupancy is what a queue-depth alarm reads, and peek lets the strategy
    // inspect the oldest tick before deciding to consume it.
    let oldest = rx.peek().expect("ring is full").seq;
    println!(
        "  depth {}/{} full={}, oldest queued seq {oldest}",
        rx.len(),
        rx.capacity(),
        tx.is_full()
    );
    println!("  dropped {} stale ticks on resync", rx.clear());

    // Steady state: a drained ring loses nothing and preserves feed order.
    let n = 50_000u64;
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<Tick>(1024);
    let feed = thread::spawn(move || {
        for seq in 0..n {
            let tick = Tick {
                seq,
                price_cents: 10_000 + (seq % 500) as u32,
            };
            while tx.try_push(tick).is_err() {
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
    let received = strategy.join().unwrap();
    println!("  streamed {received} ticks in order, zero loss when drained");
    assert_eq!(received, n);
}

/// `bulk` feature: a feed handler often lifts a whole batch of ticks off one
/// NIC receive. `try_enqueue_bulk` copies the run in behind a single release
/// fence instead of one per tick; the strategy drains behind one acquire fence.
#[cfg(feature = "bulk")]
fn bulk_batch_ingest() {
    println!("\n== bulk: batch a NIC receive into the ring ==");
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<Tick>(16);
    let batch: Vec<Tick> = (0..10)
        .map(|seq| Tick {
            seq,
            price_cents: 20_000 + seq as u32,
        })
        .collect();

    let pushed = tx.try_enqueue_bulk(&batch);
    println!(
        "  offered {} ticks, took {pushed} in one fenced call",
        batch.len()
    );
    assert_eq!(pushed, 10);

    let mut out = [Tick {
        seq: 0,
        price_cents: 0,
    }; 10];
    let drained = rx.try_dequeue_bulk(&mut out);
    println!("  drained {drained} in one fenced call");
    assert_eq!(drained, 10);
    assert_eq!(out.as_slice(), batch.as_slice(), "bulk preserves order");
}

/// `wait-strategies` feature: wrap the non-blocking ends in a blocking handle
/// with a backoff policy. `YieldStrategy` lets other threads run between
/// retries, a sensible default when the feed and strategy share cores.
#[cfg(feature = "wait-strategies")]
fn blocking_handoff() {
    use subms_spsc_ring_buffer::{BlockingSpscConsumer, BlockingSpscProducer, YieldStrategy};

    println!("\n== wait-strategies: blocking handoff (yield backoff) ==");
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
            let tick = consumer.pop();
            assert_eq!(tick.seq, expected, "blocking pop keeps feed order");
        }
        n
    });
    feed.join().unwrap();
    let got = strategy.join().unwrap();
    println!("  handed off {got} ticks, producer blocked on full instead of dropping");
    assert_eq!(got, n);
}

/// `mpsc-fan-in` feature: several venue feeds, one strategy consumer. Each feed
/// owns an independent SPSC ring (still wait-free against its own counter); the
/// consumer round-robins so a quiet venue never starves a busy one.
#[cfg(feature = "mpsc-fan-in")]
fn many_venue_fan_in() {
    use subms_spsc_ring_buffer::MpscFanIn;

    println!("\n== mpsc-fan-in: three venue feeds -> one strategy ==");
    let venues = 3usize;
    let per_venue = 20_000u64;
    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<Tick>(venues, 256);

    let mut feeds = Vec::new();
    for venue in 0..venues {
        let mut p = producers.remove(0);
        feeds.push(thread::spawn(move || {
            for seq in 0..per_venue {
                let tick = Tick {
                    seq,
                    price_cents: 40_000 + venue as u32,
                };
                while p.try_push(tick).is_err() {
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
    let got = strategy.join().unwrap();
    println!("  {venues} feeds x {per_venue} ticks -> consumer drained {got}");
    assert_eq!(got, total);
}

/// `mpmc-disruptor` feature: broadcast one tick stream to independent readers,
/// a strategy and a risk monitor, each of which sees every published tick. (To
/// have each tick handled by exactly one reader, reach for `mpsc-fan-in`.)
#[cfg(feature = "mpmc-disruptor")]
fn broadcast_to_strategy_and_risk() {
    use subms_spsc_ring_buffer::MpmcDisruptor;

    println!("\n== mpmc-disruptor: broadcast to strategy + risk ==");
    let n = 8u64;
    let (producer, mut consumers) = MpmcDisruptor::with_consumers::<Tick>(16, 2);
    let (strategy, rest) = consumers.split_at_mut(1);
    let strategy = &mut strategy[0];
    let risk = &mut rest[0];

    // Small and single-threaded so the tour self-verifies; the threaded
    // broadcast path is pinned in the tests.
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
    println!(
        "  published {published}; strategy saw {}, risk saw {}",
        strat_seen.len(),
        risk_seen.len()
    );
    let expected: Vec<u64> = (0..n).collect();
    assert_eq!(strat_seen, expected, "strategy sees every tick");
    assert_eq!(risk_seen, expected, "risk monitor sees every tick too");
}

/// `metrics` feature: wrap a base pair to count enqueue/dequeue success + fail
/// and track the high-water depth - operational stats for a feed, at the cost
/// of one atomic increment per op.
#[cfg(feature = "metrics")]
fn instrumented_handoff() {
    use subms_spsc_ring_buffer::InstrumentedSpsc;

    println!("\n== metrics: instrumented feed handoff ==");
    let (tx, rx) = SpscRingBuffer::with_capacity::<Tick>(4);
    let (mut tx, mut rx, metrics) = InstrumentedSpsc::wrap(tx, rx);

    // Fill to capacity, one overflow drop, then drain plus one empty poll.
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
    println!(
        "  enqueued {} (dropped {}), dequeued {} (empty {}), peak depth {}",
        snap.enqueue_success,
        snap.enqueue_fail,
        snap.dequeue_success,
        snap.dequeue_fail,
        snap.max_depth_observed
    );
    assert_eq!(snap.enqueue_success, 4);
    assert_eq!(snap.enqueue_fail, 1);
    assert_eq!(snap.dequeue_success, 4);
    assert_eq!(snap.dequeue_fail, 1);
    assert_eq!(snap.max_depth_observed, 4);
}
