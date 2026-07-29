//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the fan-in scenario loses no order and preserves per-gateway FIFO, and each
//! opt-in feature holds its own contract. Feature-gated the same way as the
//! sample; std-only, not harness-gated.

use std::sync::Arc;
use std::thread;

use subms_mpsc_queue::{MpscQueue, PopResult};

fn order_id(gateway: usize, seq: usize) -> u64 {
    ((gateway as u64) << 32) | seq as u64
}

#[test]
fn order_fan_in_loses_nothing_and_keeps_gateway_order() {
    const GATEWAYS: usize = 4;
    const ORDERS_PER_GATEWAY: usize = 1_000;

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
                    assert!(seq > prev, "per-gateway FIFO preserved");
                }
                last_seq[g] = Some(seq);
                per_gateway[g] += 1;
                matched += 1;
            }
            PopResult::Inconsistent => continue,
            PopResult::Empty => break,
        }
    }

    assert_eq!(matched, total, "every order matched exactly once");
    for count in per_gateway {
        assert_eq!(count, ORDERS_PER_GATEWAY, "every gateway fully drained");
    }
}

#[cfg(feature = "mpmc")]
#[test]
fn mpmc_shards_drain_every_order_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use subms_mpsc_queue::MpmcQueue;

    const GATEWAYS: usize = 4;
    const ORDERS_PER_GATEWAY: usize = 2_000;
    let shards = 3usize;
    let total = GATEWAYS * ORDERS_PER_GATEWAY;

    let ring: Arc<MpmcQueue<u64>> = Arc::new(MpmcQueue::new(1_024));
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
    assert_eq!(
        drained, total,
        "shards together drain every order exactly once"
    );
}

#[cfg(feature = "bounded")]
#[test]
fn bounded_inbox_sheds_when_full_then_reopens() {
    use subms_mpsc_queue::BoundedMpscQueue;

    let mut inbox: BoundedMpscQueue<u64> = BoundedMpscQueue::new(4);
    let cap = inbox.capacity();

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for seq in 0..cap + 2 {
        match inbox.try_enqueue(order_id(0, seq)) {
            Ok(()) => accepted += 1,
            Err(_) => rejected += 1,
        }
    }
    assert_eq!(accepted, cap, "accepts exactly one full ring");
    assert_eq!(rejected, 2, "overflow is handed back");

    assert!(inbox.try_dequeue().is_some());
    assert!(
        inbox.try_enqueue(order_id(0, 99)).is_ok(),
        "a freed slot reopens the inbox"
    );
}

#[cfg(feature = "batch")]
#[test]
fn batch_drains_full_ticks_until_the_tail() {
    use subms_mpsc_queue::BatchMpscQueue;

    const TICK: usize = 256;
    let total = 1_000usize;
    let mut q: BatchMpscQueue<u64> = BatchMpscQueue::new();
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
    assert_eq!(matched, total, "every queued order is drained");
    assert_eq!(ticks, total.div_ceil(TICK));
}

#[cfg(feature = "metrics")]
#[test]
fn metrics_snapshot_counts_pushes_and_pops() {
    use subms_mpsc_queue::MetricsMpscQueue;

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
    let _ = q.try_pop();

    let snap = q.snapshot();
    assert_eq!(snap.enqueue_ok, 500);
    assert_eq!(snap.dequeue_ok, 500);
    assert!(snap.dequeue_fail >= 1);
}

#[cfg(feature = "affinity")]
#[test]
fn affinity_rejects_empty_and_returns_a_result() {
    use subms_mpsc_queue::{AffinityError, set_affinity};

    assert!(matches!(
        set_affinity(&[]),
        Err(AffinityError::InvalidCore(0))
    ));
    // Core 0 exists on every affinity-capable platform; a container sandbox may
    // still deny it, so accept OK or an explicit OS error, never a panic.
    let result = set_affinity(&[0]);
    assert!(
        matches!(
            result,
            Ok(()) | Err(AffinityError::OsError(_)) | Err(AffinityError::Unsupported)
        ),
        "unexpected affinity result: {result:?}"
    );
}
