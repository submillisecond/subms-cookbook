//! Sample app: a tour of `subms-mpsc-queue` in an order-entry setting - N
//! gateway threads funnel orders into one matching-engine consumer. Run the
//! base with `cargo run --example sample_app`; add `--all-features` (or a
//! subset like `--features bounded`) to light up the feature sections.
//!
//! * base     - N order-entry gateways fan in to one matching-engine consumer
//! * mpmc     - shard the match loop across several consumers on one ring
//! * bounded  - a fixed-capacity inbox that sheds load rather than the heap
//! * batch    - the match loop drains one tick's orders in a single fenced pass
//! * metrics  - a health snapshot of enqueue / dequeue counts
//! * affinity - pin the match loop to a core so it stops migrating

use std::sync::Arc;
use std::thread;

use subms_mpsc_queue::{MpscQueue, PopResult};

const GATEWAYS: usize = 4;
const ORDERS_PER_GATEWAY: usize = 1_000;

fn order_id(gateway: usize, seq: usize) -> u64 {
    ((gateway as u64) << 32) | seq as u64
}

fn main() {
    base_order_fan_in();

    #[cfg(feature = "mpmc")]
    mpmc_sharded_match();

    #[cfg(feature = "bounded")]
    bounded_inbox_backpressure();

    #[cfg(feature = "batch")]
    batch_drain_per_tick();

    #[cfg(feature = "metrics")]
    metrics_health_snapshot();

    #[cfg(feature = "affinity")]
    affinity_pin_match_loop();
}

/// Base API: every order-entry gateway pushes onto one shared queue and a
/// single matching-engine thread drains it. `push` is wait-free per producer;
/// the consumer tolerates the dangling-tail window by retrying on
/// `Inconsistent` rather than mistaking a mid-publish gateway for an empty
/// queue.
fn base_order_fan_in() {
    println!("== base: order-entry gateways fan in to one matching engine ==");
    let q: Arc<MpscQueue<u64>> = Arc::new(MpscQueue::new());

    let gateways: Vec<_> = (0..GATEWAYS)
        .map(|g| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for seq in 0..ORDERS_PER_GATEWAY {
                    q.push(order_id(g, seq));
                }
            })
        })
        .collect();
    for h in gateways {
        h.join().unwrap();
    }

    // Every gateway handle is joined, so the queue is uniquely owned again and
    // the single consumer can take it back out (try_pop needs &mut self).
    let mut q = Arc::into_inner(q).expect("all gateway handles dropped");

    let total = GATEWAYS * ORDERS_PER_GATEWAY;
    let mut per_gateway = [0usize; GATEWAYS];
    let mut last_seq = [None::<u64>; GATEWAYS];
    let mut matched = 0usize;
    loop {
        match q.try_pop() {
            PopResult::Some(order) => {
                let g = (order >> 32) as usize;
                let seq = order & 0xffff_ffff;
                if let Some(prev) = last_seq[g] {
                    assert!(seq > prev, "orders from one gateway stay in FIFO order");
                }
                last_seq[g] = Some(seq);
                per_gateway[g] += 1;
                matched += 1;
            }
            PopResult::Inconsistent => continue,
            PopResult::Empty => break,
        }
    }

    println!("  {GATEWAYS} gateways x {ORDERS_PER_GATEWAY} orders -> matched {matched}");
    println!("  per-gateway tally: {per_gateway:?}");
    assert_eq!(matched, total, "no order dropped, none duplicated");
    for count in per_gateway {
        assert_eq!(count, ORDERS_PER_GATEWAY, "every gateway fully drained");
    }
}

/// `mpmc` feature: the base queue allows one consumer. When one match loop
/// cannot keep up, `MpmcQueue` is a bounded Disruptor-style ring where several
/// consumer shards race the head; the loser of a CAS refreshes and retries.
/// Producers race the tail the same way.
#[cfg(feature = "mpmc")]
fn mpmc_sharded_match() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use subms_mpsc_queue::MpmcQueue;

    println!("\n== mpmc: shard the match loop across several consumers ==");
    let shards = 3usize;
    let ring: Arc<MpmcQueue<u64>> = Arc::new(MpmcQueue::new(1_024));
    let total = GATEWAYS * ORDERS_PER_GATEWAY;

    let gateways: Vec<_> = (0..GATEWAYS)
        .map(|g| {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                for seq in 0..ORDERS_PER_GATEWAY {
                    let mut order = order_id(g, seq);
                    while let Err(rejected) = ring.try_enqueue(order) {
                        order = rejected;
                        std::hint::spin_loop();
                    }
                }
            })
        })
        .collect();

    let matched = Arc::new(AtomicUsize::new(0));
    let consumers: Vec<_> = (0..shards)
        .map(|_| {
            let ring = Arc::clone(&ring);
            let matched = Arc::clone(&matched);
            thread::spawn(move || {
                let mut local = 0usize;
                loop {
                    if ring.try_dequeue().is_some() {
                        local += 1;
                        matched.fetch_add(1, Ordering::Relaxed);
                    } else if matched.load(Ordering::Relaxed) >= total {
                        break;
                    } else {
                        std::hint::spin_loop();
                    }
                }
                local
            })
        })
        .collect();

    for h in gateways {
        h.join().unwrap();
    }
    let drained: usize = consumers.into_iter().map(|c| c.join().unwrap()).sum();
    println!(
        "  {shards} shards drained {drained} orders (cas retries: {})",
        ring.cas_retries()
    );
    assert_eq!(
        drained, total,
        "shards together drain every order exactly once"
    );
}

/// `bounded` feature: a fixed-capacity ring gives the gateway backpressure.
/// On the base queue a slow match loop turns into unbounded heap growth; the
/// bounded inbox returns the rejected order so the gateway can retry or shed
/// load instead of the backlog landing on the heap.
#[cfg(feature = "bounded")]
fn bounded_inbox_backpressure() {
    use subms_mpsc_queue::BoundedMpscQueue;

    println!("\n== bounded: a fixed-capacity inbox that pushes back ==");
    let mut inbox: BoundedMpscQueue<u64> = BoundedMpscQueue::new(4);
    let cap = inbox.capacity();

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for seq in 0..cap + 2 {
        match inbox.try_enqueue(order_id(0, seq)) {
            Ok(()) => accepted += 1,
            Err(_order) => rejected += 1,
        }
    }
    println!("  capacity {cap}: accepted {accepted}, shed {rejected} while full");
    assert_eq!(accepted, cap, "accepts exactly one full ring");
    assert_eq!(rejected, 2, "the overflow is handed back, not queued");

    // Drain one, and a previously-rejected order now fits.
    assert!(
        inbox.try_dequeue().is_some(),
        "match loop consumes one order"
    );
    assert!(
        inbox.try_enqueue(order_id(0, 99)).is_ok(),
        "a freed slot reopens the inbox"
    );
}

/// `batch` feature: a match loop that runs on a tick drains a whole tick's
/// worth of orders in one fenced pass. `try_dequeue_batch` pays one acquire
/// per call instead of one per order and stops early on empty or a mid-publish
/// gateway.
#[cfg(feature = "batch")]
fn batch_drain_per_tick() {
    use subms_mpsc_queue::BatchMpscQueue;

    println!("\n== batch: drain one tick's orders in a single pass ==");
    const TICK: usize = 256;
    let mut q: BatchMpscQueue<u64> = BatchMpscQueue::new();
    let total = 1_000usize;
    for seq in 0..total {
        q.push(order_id(0, seq));
    }

    let mut buf: Vec<Option<u64>> = (0..TICK).map(|_| None).collect();
    let mut ticks = 0usize;
    let mut matched = 0usize;
    loop {
        let n = q.try_dequeue_batch(&mut buf);
        if n == 0 {
            break;
        }
        ticks += 1;
        for slot in buf.iter_mut().take(n) {
            let _ = slot.take();
            matched += 1;
        }
    }
    println!("  drained {matched} orders across {ticks} ticks of up to {TICK}");
    assert_eq!(matched, total, "every queued order is drained");
    assert_eq!(
        ticks,
        total.div_ceil(TICK),
        "each tick drains a full buffer until the tail"
    );
}

/// `metrics` feature: wrap the queue in per-instance counters to answer "is
/// this inbox actually contended" from a health snapshot rather than a guess.
/// The counters are relaxed - advisory, not part of the queue's correctness.
#[cfg(feature = "metrics")]
fn metrics_health_snapshot() {
    use subms_mpsc_queue::MetricsMpscQueue;

    println!("\n== metrics: a health snapshot of the inbox ==");
    let mut q: MetricsMpscQueue<u64> = MetricsMpscQueue::new();
    for seq in 0..500 {
        q.push(order_id(0, seq));
    }
    let mut matched = 0usize;
    while matched < 500 {
        match q.try_pop() {
            PopResult::Some(_) => matched += 1,
            PopResult::Inconsistent => continue,
            PopResult::Empty => break,
        }
    }
    // One extra pop on the drained queue to register a dequeue miss.
    let _ = q.try_pop();

    let snap = q.snapshot();
    println!(
        "  enqueue_ok={} dequeue_ok={} dequeue_fail={}",
        snap.enqueue_ok, snap.dequeue_ok, snap.dequeue_fail
    );
    assert_eq!(snap.enqueue_ok, 500, "every push is counted");
    assert_eq!(snap.dequeue_ok, 500, "every matched order is counted");
    assert!(
        snap.dequeue_fail >= 1,
        "the miss on the drained queue is counted"
    );
}

/// `affinity` feature: pin the match loop to a core so it stops migrating and
/// trashing the producers' cache lines. Real on Linux and Windows; a documented
/// `Unsupported` no-op elsewhere. An empty core set is always rejected.
#[cfg(feature = "affinity")]
fn affinity_pin_match_loop() {
    use subms_mpsc_queue::{AffinityError, set_affinity};

    println!("\n== affinity: pin the match loop to a core ==");
    match set_affinity(&[0]) {
        Ok(()) => println!("  match loop pinned to core 0"),
        Err(e) => println!("  pinning unavailable: {e}"),
    }
    assert!(
        matches!(set_affinity(&[]), Err(AffinityError::InvalidCore(0))),
        "an empty core set is always rejected"
    );
}
