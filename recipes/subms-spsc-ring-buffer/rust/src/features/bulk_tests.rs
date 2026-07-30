use std::thread;

use crate::SpscRingBuffer;

#[test]
fn bulk_enqueue_then_bulk_dequeue_round_trip() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(16);
    let src: Vec<u32> = (0..10).collect();
    let n = tx.try_enqueue_bulk(&src);
    assert_eq!(n, 10);
    let mut out = [0u32; 10];
    let m = rx.try_dequeue_bulk(&mut out);
    assert_eq!(m, 10);
    assert_eq!(out, src.as_slice());
}

#[test]
fn bulk_enqueue_partial_when_ring_almost_full() {
    let (mut tx, mut _rx) = SpscRingBuffer::with_capacity::<u32>(4);
    // Capacity 4: push 3 single items, then try to bulk-push 5 more.
    for i in 0..3u32 {
        tx.try_push(i).unwrap();
    }
    let src = [100u32, 101, 102, 103, 104];
    let n = tx.try_enqueue_bulk(&src);
    // Only 1 slot free.
    assert_eq!(n, 1);
}

#[test]
fn bulk_dequeue_partial_when_ring_almost_empty() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(8);
    for i in 0..3u32 {
        tx.try_push(i).unwrap();
    }
    let mut out = [0u32; 10];
    let n = rx.try_dequeue_bulk(&mut out);
    assert_eq!(n, 3);
    assert_eq!(&out[..3], &[0u32, 1, 2]);
}

#[test]
fn bulk_returns_zero_on_full_ring() {
    let (mut tx, _rx) = SpscRingBuffer::with_capacity::<u32>(4);
    for i in 0..4u32 {
        tx.try_push(i).unwrap();
    }
    let n = tx.try_enqueue_bulk(&[99u32, 100]);
    assert_eq!(n, 0);
}

#[test]
fn bulk_returns_zero_on_empty_ring() {
    let (_tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut out = [0u32; 4];
    let n = rx.try_dequeue_bulk(&mut out);
    assert_eq!(n, 0);
}

#[test]
fn bulk_handles_zero_length_slice() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    assert_eq!(tx.try_enqueue_bulk(&[]), 0);
    let mut out: [u32; 0] = [];
    assert_eq!(rx.try_dequeue_bulk(&mut out), 0);
}

#[test]
fn bulk_wraps_around_correctly() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(8);
    // Drain to start at index 6 - close to the wrap boundary.
    for i in 0..6u32 {
        tx.try_push(i).unwrap();
    }
    for _ in 0..6 {
        rx.try_pop();
    }
    // Now push 6 - which has to wrap from index 6 across 7 -> 0,1,2,3.
    let src: Vec<u32> = (10..16).collect();
    let n = tx.try_enqueue_bulk(&src);
    assert_eq!(n, 6);
    let mut out = [0u32; 6];
    let m = rx.try_dequeue_bulk(&mut out);
    assert_eq!(m, 6);
    assert_eq!(out, src.as_slice());
}

#[test]
fn bulk_repeated_wrap_cycles_single_thread() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut expected = 0u32;
    for _ in 0..8 {
        let src = [expected, expected + 1, expected + 2, expected + 3];
        let n = tx.try_enqueue_bulk(&src);
        assert_eq!(n, 4);
        let mut out = [0u32; 4];
        let m = rx.try_dequeue_bulk(&mut out);
        assert_eq!(m, 4);
        assert_eq!(out, src);
        expected += 4;
    }
}

#[test]
fn bulk_dequeue_into_smaller_slice_than_available() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(16);
    let src: Vec<u32> = (0..12).collect();
    assert_eq!(tx.try_enqueue_bulk(&src), 12);
    let mut out = [0u32; 5];
    assert_eq!(rx.try_dequeue_bulk(&mut out), 5);
    assert_eq!(out, [0u32, 1, 2, 3, 4]);
    let mut rest = [0u32; 16];
    assert_eq!(rx.try_dequeue_bulk(&mut rest), 7);
    assert_eq!(&rest[..7], &[5u32, 6, 7, 8, 9, 10, 11]);
}

// Modest colocated concurrency check; the multi-hundred-thousand-item
// variant lives in tests/stress.rs so the coverage --lib run stays fast.
#[test]
fn bulk_round_trip_under_two_threads() {
    let n_items = 20_000u32;
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
