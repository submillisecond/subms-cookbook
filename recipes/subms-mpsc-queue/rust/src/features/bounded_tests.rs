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
