//! Heavy multi-thread stress tests. These live in the integration target so a
//! coverage run (`cargo tarpaulin --lib`) skips them - their multi-million-op
//! ping-pongs are far too slow under debug instrumentation and would trip the
//! coverage harness timeout. They still run under a normal `cargo test`.

use std::thread;

use subms_spsc_ring_buffer::SpscRingBuffer;

#[test]
fn round_trip_holds_under_two_threads() {
    let n = 1_000_000u64;
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u64>(1024);

    let producer = thread::spawn(move || {
        let mut i = 0u64;
        while i < n {
            if tx.try_push(i).is_ok() {
                i += 1;
            }
        }
    });

    let consumer = thread::spawn(move || {
        let mut next = 0u64;
        while next < n {
            if let Some(v) = rx.try_pop() {
                assert_eq!(v, next, "out-of-order or lost item at {next}");
                next += 1;
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}

#[cfg(feature = "bulk")]
#[test]
fn bulk_round_trip_under_two_threads_heavy() {
    let n_items = 200_000u32;
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(1024);

    let producer = thread::spawn(move || {
        let mut sent = 0u32;
        while sent < n_items {
            let remaining = n_items - sent;
            let take = 32u32.min(remaining);
            let buf: Vec<u32> = (sent..sent + take).collect();
            let pushed = tx.try_enqueue_bulk(&buf) as u32;
            sent += pushed;
        }
    });

    let consumer = thread::spawn(move || {
        let mut next = 0u32;
        let mut buf = [0u32; 32];
        while next < n_items {
            let m = rx.try_dequeue_bulk(&mut buf);
            for &v in &buf[..m] {
                assert_eq!(v, next, "out of order at {next}");
                next += 1;
            }
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();
}

#[cfg(feature = "mpsc-fan-in")]
#[test]
fn three_producers_one_consumer_under_threads_heavy() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use subms_spsc_ring_buffer::MpscFanIn;

    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u64>(3, 256);
    let per_producer = 50_000u64;
    let consumed = Arc::new(AtomicUsize::new(0));
    let consumed_c = consumed.clone();
    let total = per_producer * 3;

    let consumer_t = thread::spawn(move || {
        let mut local = 0u64;
        while local < total {
            if consumer.try_pop().is_some() {
                local += 1;
                consumed_c.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let mut handles = Vec::new();
    for (i, _) in (0..3).enumerate() {
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
