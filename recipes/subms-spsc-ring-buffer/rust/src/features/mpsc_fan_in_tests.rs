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
fn producer_count_and_capacity_reported() {
    let (producers, consumer) = MpscFanIn::with_capacity::<u32>(5, 6);
    assert_eq!(consumer.producer_count(), 5);
    // per_ring_capacity 6 rounds up to the next power of two.
    assert_eq!(producers[0].capacity(), 8);
}

// Modest colocated concurrency check; the larger fan-in stress lives in
// tests/stress.rs so the coverage --lib run stays fast.
#[test]
fn three_producers_one_consumer_under_threads() {
    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u64>(3, 256);
    let per_producer = 5_000u64;
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

#[test]
fn interleaved_fill_and_drain_single_thread() {
    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(3, 4);
    let mut got = Vec::new();
    for round in 0..4u32 {
        for (i, p) in producers.iter_mut().enumerate() {
            p.try_push(round * 10 + i as u32).unwrap();
        }
        while let Some(v) = consumer.try_pop() {
            got.push(v);
        }
    }
    assert_eq!(got.len(), 12);
}

#[test]
fn producer_backpressure_reports_full_ring() {
    let (mut producers, mut consumer) = MpscFanIn::with_capacity::<u32>(1, 2);
    assert!(producers[0].try_push(1).is_ok());
    assert!(producers[0].try_push(2).is_ok());
    // Capacity 2 is now full; the third push is rejected with the value back.
    assert_eq!(producers[0].try_push(3), Err(3));
    assert_eq!(consumer.try_pop(), Some(1));
    assert!(producers[0].try_push(3).is_ok());
}
