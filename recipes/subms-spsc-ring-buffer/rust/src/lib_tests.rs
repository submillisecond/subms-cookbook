use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::*;

#[test]
fn push_and_pop_a_single_value() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    tx.try_push(7).unwrap();
    assert_eq!(rx.try_pop(), Some(7));
    assert_eq!(rx.try_pop(), None);
}

#[test]
fn pop_on_empty_returns_none() {
    let (_tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    assert_eq!(rx.try_pop(), None);
    assert_eq!(rx.try_pop(), None);
}

#[test]
fn fills_to_capacity_then_rejects() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let cap = tx.capacity();
    for i in 0..cap as u32 {
        tx.try_push(i).expect("slot available");
    }
    let next = tx.try_push(999u32).unwrap_err();
    assert_eq!(next, 999);
    for i in 0..cap as u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    assert_eq!(rx.try_pop(), None);
}

#[test]
fn alternating_push_pop_round_trip() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    for i in 0..100u32 {
        tx.try_push(i).unwrap();
        assert_eq!(rx.try_pop(), Some(i));
    }
    assert_eq!(rx.try_pop(), None);
}

#[test]
fn capacity_is_rounded_up_to_power_of_two() {
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u8>(5);
    assert_eq!(tx.capacity(), 8);
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u8>(0);
    assert_eq!(tx.capacity(), 2);
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u8>(1);
    assert_eq!(tx.capacity(), 2);
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u8>(16);
    assert_eq!(tx.capacity(), 16);
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u8>(17);
    assert_eq!(tx.capacity(), 32);
}

#[test]
fn capacity_one_request_still_has_two_slots() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(1);
    assert_eq!(tx.capacity(), 2);
    tx.try_push(1).unwrap();
    tx.try_push(2).unwrap();
    assert!(tx.try_push(3).is_err());
    assert_eq!(rx.try_pop(), Some(1));
    assert_eq!(rx.try_pop(), Some(2));
}

#[test]
fn consumer_reports_the_same_capacity_as_producer() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(17);
    assert_eq!(rx.capacity(), 32);
    assert_eq!(rx.capacity(), tx.capacity());
}

#[test]
fn wraps_around_the_ring_correctly() {
    // Capacity 4: push 4, pop 4, push 4 more, pop 4 - tests wrap.
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    for i in 0..4u32 {
        tx.try_push(i).unwrap();
    }
    for i in 0..4u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    for i in 4..8u32 {
        tx.try_push(i).unwrap();
    }
    for i in 4..8u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    assert_eq!(rx.try_pop(), None);
}

#[test]
fn full_then_partial_drain_then_refill() {
    // Verifies opposite-index caching doesn't lose entries on partial drain.
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(8);
    for i in 0..8u32 {
        tx.try_push(i).unwrap();
    }
    for i in 0..4u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    for i in 100..104u32 {
        tx.try_push(i).unwrap();
    }
    for i in 4..8u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    for i in 100..104u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    assert_eq!(rx.try_pop(), None);
}

// The multi-million-op two-thread round trip lives in tests/stress.rs so the
// coverage --lib run stays fast; this modest tight-ring variant keeps the
// concurrent full/empty transitions covered here.
#[test]
fn round_trip_with_small_capacity_under_threads() {
    // Tight ring forces frequent full/empty transitions.
    let n = 20_000u64;
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u64>(8);
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
                assert_eq!(v, next);
                next += 1;
            }
        }
    });
    producer.join().unwrap();
    consumer.join().unwrap();
}

#[derive(Debug)]
struct DropCounted(Arc<AtomicUsize>);
impl Drop for DropCounted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn drops_pending_items_on_destruction() {
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let (mut tx, _rx) = SpscRingBuffer::with_capacity::<DropCounted>(4);
        tx.try_push(DropCounted(counter.clone())).unwrap();
        tx.try_push(DropCounted(counter.clone())).unwrap();
        tx.try_push(DropCounted(counter.clone())).unwrap();
    }
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}

#[test]
fn popped_items_drop_only_once() {
    let counter = Arc::new(AtomicUsize::new(0));
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<DropCounted>(4);
    tx.try_push(DropCounted(counter.clone())).unwrap();
    let v = rx.try_pop().unwrap();
    drop(v);
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}

#[test]
fn full_drain_then_refill_after_long_pause() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    for i in 0..4u32 {
        tx.try_push(i).unwrap();
    }
    for i in 0..4u32 {
        assert_eq!(rx.try_pop(), Some(i));
    }
    // Stall: many no-op pops should all return None.
    for _ in 0..1000 {
        assert_eq!(rx.try_pop(), None);
    }
    tx.try_push(42).unwrap();
    assert_eq!(rx.try_pop(), Some(42));
}

#[test]
fn occupancy_tracks_pushes_and_pops_from_both_handles() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    assert_eq!(tx.len(), 0);
    assert!(tx.is_empty());
    assert!(rx.is_empty());
    assert!(!tx.is_full());

    for i in 0..4u32 {
        tx.try_push(i).unwrap();
    }
    assert_eq!(tx.len(), 4);
    assert_eq!(rx.len(), 4);
    assert!(tx.is_full());
    assert!(rx.is_full());
    assert!(!rx.is_empty());

    rx.try_pop().unwrap();
    assert_eq!(tx.len(), 3);
    assert!(!tx.is_full());
}

#[test]
fn occupancy_is_correct_after_the_counters_wrap_the_slot_array() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    for round in 0..100u32 {
        tx.try_push(round).unwrap();
        tx.try_push(round).unwrap();
        assert_eq!(tx.len(), 2);
        rx.try_pop().unwrap();
        rx.try_pop().unwrap();
        assert_eq!(rx.len(), 0);
    }
}

#[test]
fn peek_returns_the_head_without_consuming_it() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    assert_eq!(rx.peek(), None);
    tx.try_push(11).unwrap();
    tx.try_push(22).unwrap();
    assert_eq!(rx.peek(), Some(&11));
    assert_eq!(rx.peek(), Some(&11));
    assert_eq!(rx.len(), 2);
    assert_eq!(rx.try_pop(), Some(11));
    assert_eq!(rx.peek(), Some(&22));
}

#[test]
fn peek_refreshes_the_cached_tail_after_an_empty_observation() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(4);
    assert_eq!(rx.peek(), None);
    tx.try_push(5).unwrap();
    assert_eq!(rx.peek(), Some(&5));
}

#[test]
fn clear_drops_every_buffered_item_and_reports_the_count() {
    let counter = Arc::new(AtomicUsize::new(0));
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<DropCounted>(4);
    for _ in 0..3 {
        tx.try_push(DropCounted(counter.clone())).unwrap();
    }
    assert_eq!(rx.clear(), 3);
    assert_eq!(counter.load(Ordering::Relaxed), 3);
    assert!(rx.is_empty());
    assert_eq!(rx.clear(), 0);

    // The ring is reusable after a clear - slots were freed, not leaked.
    tx.try_push(DropCounted(counter.clone())).unwrap();
    assert_eq!(rx.len(), 1);
}
