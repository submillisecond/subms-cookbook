use std::thread;

use super::*;
use crate::SpscRingBuffer;

#[test]
fn counts_enqueue_success_and_fail() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(2);
    let (mut tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
    tx.try_push(1).unwrap();
    tx.try_push(2).unwrap();
    assert!(tx.try_push(3).is_err());
    let s = m.snapshot();
    assert_eq!(s.enqueue_success, 2);
    assert_eq!(s.enqueue_fail, 1);
}

#[test]
fn counts_dequeue_success_and_fail() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let (mut tx, mut rx, m) = InstrumentedSpsc::wrap(tx, rx);
    tx.try_push(7).unwrap();
    assert_eq!(rx.try_pop(), Some(7));
    assert_eq!(rx.try_pop(), None);
    assert_eq!(rx.try_pop(), None);
    let s = m.snapshot();
    assert_eq!(s.dequeue_success, 1);
    assert_eq!(s.dequeue_fail, 2);
}

#[test]
fn tracks_max_depth_observed() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let (mut tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
    for i in 0..4u32 {
        tx.try_push(i).unwrap();
    }
    let s = m.snapshot();
    assert_eq!(s.max_depth_observed, 4);
}

#[test]
fn producer_and_consumer_report_capacity() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(6);
    let (tx, rx, _m) = InstrumentedSpsc::wrap(tx, rx);
    assert_eq!(tx.capacity(), 8);
    assert_eq!(rx.capacity(), 8);
}

#[test]
fn default_ring_metrics_is_zeroed() {
    let m = RingMetrics::default();
    assert_eq!(m.snapshot(), RingMetricsSnapshot::default());
}

#[test]
fn snapshot_returns_consistent_zeros_initially() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let (_tx, _rx, m) = InstrumentedSpsc::wrap(tx, rx);
    let s = m.snapshot();
    assert_eq!(
        s,
        RingMetricsSnapshot {
            enqueue_success: 0,
            enqueue_fail: 0,
            dequeue_success: 0,
            dequeue_fail: 0,
            max_depth_observed: 0,
            cas_retries: 0,
        }
    );
}

#[test]
fn cas_retry_counter_is_writeable() {
    let m = RingMetrics::new();
    for _ in 0..7 {
        m.record_cas_retry();
    }
    assert_eq!(m.snapshot().cas_retries, 7);
}

#[test]
fn metrics_observed_across_threads() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(64);
    let (mut tx, mut rx, m) = InstrumentedSpsc::wrap(tx, rx);
    let n = 5_000u64;
    let producer = thread::spawn(move || {
        for i in 0..n {
            while tx.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });
    let consumer = thread::spawn(move || {
        let mut got = 0u64;
        while got < n {
            if rx.try_pop().is_some() {
                got += 1;
            }
        }
    });
    producer.join().unwrap();
    consumer.join().unwrap();
    let s = m.snapshot();
    assert_eq!(s.enqueue_success, n);
    assert_eq!(s.dequeue_success, n);
}
