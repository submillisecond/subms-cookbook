use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

#[test]
fn enqueue_dequeue_single_item() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
    assert!(q.try_enqueue(42).is_ok());
    assert_eq!(q.try_dequeue(), Some(42));
    assert_eq!(q.try_dequeue(), None);
}

#[test]
fn capacity_is_power_of_two() {
    let q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(5);
    assert_eq!(q.capacity(), 8);
    let q2: BoundedMpscQueue<u32> = BoundedMpscQueue::new(1);
    assert_eq!(q2.capacity(), 2);
}

#[test]
fn enqueue_full_returns_value_back() {
    let q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
    for i in 0..4 {
        assert!(q.try_enqueue(i).is_ok());
    }
    // 4 cap, all slots in use.
    let rejected = q.try_enqueue(99);
    assert_eq!(rejected, Err(99));
}

#[test]
fn fifo_order_single_producer() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(16);
    for i in 0..16 {
        assert!(q.try_enqueue(i).is_ok());
    }
    for i in 0..16 {
        assert_eq!(q.try_dequeue(), Some(i));
    }
}

#[test]
fn drain_then_refill_wraps_ring() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
    for round in 0..10 {
        for i in 0..4 {
            assert!(q.try_enqueue(round * 4 + i).is_ok());
        }
        for i in 0..4 {
            assert_eq!(q.try_dequeue(), Some(round * 4 + i));
        }
    }
}

#[test]
fn multi_producer_no_lost_items() {
    let producers = 4usize;
    let per_producer = 5_000usize;
    let q: Arc<BoundedMpscQueue<u64>> = Arc::new(BoundedMpscQueue::new(1024));
    let stop = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for tid in 0..producers as u64 {
        let q = q.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0u64;
            while i < per_producer as u64 {
                if q.try_enqueue((tid << 32) | i).is_ok() {
                    i += 1;
                } else {
                    std::hint::spin_loop();
                }
            }
        }));
    }
    let cq = q.clone();
    let cstop = stop.clone();
    let consumer = thread::spawn(move || {
        // Manual pointer trick to call try_dequeue (&mut self) on a
        // shared Arc; only one consumer thread exists.
        let qp = Arc::as_ptr(&cq) as *mut BoundedMpscQueue<u64>;
        let qm = unsafe { &mut *qp };
        let mut counts = [0u64; 4];
        let mut total = 0usize;
        while total < producers * per_producer {
            if let Some(v) = qm.try_dequeue() {
                counts[(v >> 32) as usize] += 1;
                total += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        cstop.store(1, Ordering::Relaxed);
        counts
    });
    for h in handles {
        h.join().unwrap();
    }
    let counts = consumer.join().unwrap();
    for c in counts {
        assert_eq!(c as usize, per_producer);
    }
}

#[test]
fn drops_pending_items_on_destruction() {
    struct DropCounted(Arc<AtomicUsize>);
    impl Drop for DropCounted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let q: BoundedMpscQueue<DropCounted> = BoundedMpscQueue::new(4);
        assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
        assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
        assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
    }
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}

#[test]
fn len_tracks_outstanding_items() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(8);
    assert!(q.is_empty());
    q.try_enqueue(1).unwrap();
    q.try_enqueue(2).unwrap();
    assert_eq!(q.len(), 2);
    q.try_dequeue().unwrap();
    assert_eq!(q.len(), 1);
}

#[test]
fn peek_borrows_the_next_slot() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
    assert!(q.peek().is_none());
    q.try_enqueue(5).unwrap();
    q.try_enqueue(6).unwrap();
    assert_eq!(q.peek(), Some(&5));
    assert_eq!(q.peek(), Some(&5), "peek is idempotent");
    assert_eq!(q.try_dequeue(), Some(5));
    assert_eq!(q.peek(), Some(&6));
}

#[test]
fn clear_empties_the_ring_and_reopens_it() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(4);
    assert_eq!(q.clear(), 0);
    for i in 0..4 {
        q.try_enqueue(i).unwrap();
    }
    assert!(q.is_full());
    assert_eq!(q.clear(), 4);
    assert!(q.is_empty());
    assert!(!q.is_full());
    assert!(q.try_enqueue(99).is_ok(), "every slot is open again");
}

#[test]
fn is_full_flips_on_the_last_slot() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(2);
    assert!(!q.is_full());
    q.try_enqueue(1).unwrap();
    assert!(!q.is_full());
    q.try_enqueue(2).unwrap();
    assert!(q.is_full());
    assert_eq!(q.try_dequeue(), Some(1));
    assert!(!q.is_full());
}

#[test]
fn indices_are_monotonic_and_give_lag() {
    let mut q: BoundedMpscQueue<u32> = BoundedMpscQueue::new(8);
    assert_eq!((q.producer_index(), q.consumer_index()), (0, 0));
    for i in 0..5 {
        q.try_enqueue(i).unwrap();
    }
    assert_eq!(q.producer_index(), 5);
    assert_eq!(q.consumer_index(), 0);
    assert_eq!(q.producer_index() - q.consumer_index(), q.len());
    q.try_dequeue().unwrap();
    q.try_dequeue().unwrap();
    assert_eq!(
        q.consumer_index(),
        2,
        "the consumer index only moves forward"
    );
    assert_eq!(q.producer_index() - q.consumer_index(), 3);
}
