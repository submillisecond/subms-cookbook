use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::*;

#[test]
fn single_producer_single_consumer_round_trip() {
    let (p, mut consumers) = MpmcDisruptor::with_consumers::<u32>(8, 1);
    let c = &mut consumers[0];
    p.try_publish(1).unwrap();
    p.try_publish(2).unwrap();
    assert_eq!(c.try_consume(), Some(1));
    assert_eq!(c.try_consume(), Some(2));
    assert_eq!(c.try_consume(), None);
}

#[test]
fn two_consumers_both_see_every_item() {
    let (p, mut consumers) = MpmcDisruptor::with_consumers::<u32>(8, 2);
    let (c1, rest) = consumers.split_at_mut(1);
    let c1 = &mut c1[0];
    let c2 = &mut rest[0];
    for i in 0..5u32 {
        p.try_publish(i).unwrap();
    }
    for i in 0..5u32 {
        assert_eq!(c1.try_consume(), Some(i));
        assert_eq!(c2.try_consume(), Some(i));
    }
    assert_eq!(c1.try_consume(), None);
    assert_eq!(c2.try_consume(), None);
}

#[test]
fn producer_blocks_when_slowest_consumer_lags() {
    let (p, mut consumers) = MpmcDisruptor::with_consumers::<u32>(2, 1);
    let c = &mut consumers[0];
    // Fill the ring.
    for i in 0..2u32 {
        p.try_publish(i).unwrap();
    }
    // Consumer hasn't drained; next publish must fail.
    assert!(p.try_publish(99).is_err());
    // Drain one slot - now publish succeeds.
    assert_eq!(c.try_consume(), Some(0));
    p.try_publish(99).unwrap();
}

// Modest colocated concurrency check; the larger two-producer stress lives
// in tests/stress.rs so the coverage --lib run stays fast.
#[test]
fn two_producers_one_consumer_under_threads() {
    let (p, mut consumers) = MpmcDisruptor::with_consumers::<u64>(256, 1);
    let per_producer = 5_000u64;
    let count = Arc::new(AtomicUsize::new(0));
    let count_c = count.clone();

    let p1 = p.clone();
    let p2 = p.clone();
    drop(p);

    let producer1 = thread::spawn(move || {
        for i in 0..per_producer {
            let v = i;
            while p1.try_publish(v).is_err() {
                std::hint::spin_loop();
            }
        }
    });
    let producer2 = thread::spawn(move || {
        for i in 0..per_producer {
            let v = 1_000_000 + i;
            while p2.try_publish(v).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    let target = per_producer * 2;
    let consumer = thread::spawn(move || {
        let mut local = 0u64;
        let c = &mut consumers[0];
        while local < target {
            if c.try_consume().is_some() {
                local += 1;
                count_c.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    producer1.join().unwrap();
    producer2.join().unwrap();
    consumer.join().unwrap();
    assert_eq!(count.load(Ordering::Relaxed), target as usize);
}

#[test]
fn try_consume_returns_none_on_empty() {
    let (_p, mut consumers) = MpmcDisruptor::with_consumers::<u32>(4, 1);
    assert_eq!(consumers[0].try_consume(), None);
}

#[test]
fn capacity_rounded_up_to_power_of_two() {
    let (p, consumers) = MpmcDisruptor::with_consumers::<u32>(5, 1);
    assert_eq!(p.capacity(), 8);
    // Consumer reports the same capacity as the producer.
    assert_eq!(consumers[0].capacity(), 8);
    let (p, _consumers) = MpmcDisruptor::with_consumers::<u32>(1, 1);
    assert_eq!(p.capacity(), 2);
}

#[test]
fn wraps_around_correctly_across_multiple_rounds() {
    let (p, mut consumers) = MpmcDisruptor::with_consumers::<u32>(4, 1);
    let c = &mut consumers[0];
    // 4 rounds of 4 entries each, no overlap of producer/consumer.
    for round in 0..4u32 {
        for i in 0..4u32 {
            p.try_publish(round * 4 + i).unwrap();
        }
        for i in 0..4u32 {
            assert_eq!(c.try_consume(), Some(round * 4 + i));
        }
    }
}
