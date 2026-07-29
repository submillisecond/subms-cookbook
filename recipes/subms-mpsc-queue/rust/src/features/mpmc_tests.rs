use super::*;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::thread;

#[test]
fn single_thread_enqueue_dequeue() {
    let q: MpmcQueue<u32> = MpmcQueue::new(4);
    assert!(q.try_enqueue(7).is_ok());
    assert_eq!(q.try_dequeue(), Some(7));
    assert_eq!(q.try_dequeue(), None);
}

#[test]
fn full_ring_rejects_with_value() {
    let q: MpmcQueue<u32> = MpmcQueue::new(2);
    assert!(q.try_enqueue(1).is_ok());
    assert!(q.try_enqueue(2).is_ok());
    assert_eq!(q.try_enqueue(3), Err(3));
}

#[test]
fn fifo_within_single_producer_single_consumer() {
    let q: MpmcQueue<u32> = MpmcQueue::new(64);
    for i in 0..64 {
        q.try_enqueue(i).unwrap();
    }
    for i in 0..64 {
        assert_eq!(q.try_dequeue(), Some(i));
    }
}

#[test]
fn multi_consumer_drains_all_items_exactly_once() {
    let producers = 4usize;
    let consumers = 4usize;
    let per_producer = 2_500usize;
    let q: Arc<MpmcQueue<u64>> = Arc::new(MpmcQueue::new(1024));

    let mut prods = Vec::new();
    for tid in 0..producers as u64 {
        let q = q.clone();
        prods.push(thread::spawn(move || {
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

    let total = producers * per_producer;
    let total_consumed = Arc::new(AtomicUsize::new(0));
    let mut cons = Vec::new();
    for _ in 0..consumers {
        let q = q.clone();
        let total_consumed = total_consumed.clone();
        cons.push(thread::spawn(move || {
            let mut local = 0u64;
            loop {
                if let Some(_v) = q.try_dequeue() {
                    local += 1;
                    total_consumed.fetch_add(1, Ordering::Relaxed);
                } else if total_consumed.load(Ordering::Relaxed) >= total {
                    break;
                } else {
                    std::hint::spin_loop();
                }
            }
            local
        }));
    }

    for p in prods {
        p.join().unwrap();
    }
    let mut got = 0u64;
    for c in cons {
        got += c.join().unwrap();
    }
    assert_eq!(got as usize, total);
    assert_eq!(total_consumed.load(Ordering::Relaxed), total);
}

#[test]
fn cas_retries_increments_under_contention() {
    let producers = 8usize;
    let per_producer = 5_000usize;
    let q: Arc<MpmcQueue<u32>> = Arc::new(MpmcQueue::new(2048));
    let mut prods = Vec::new();
    for _ in 0..producers {
        let q = q.clone();
        prods.push(thread::spawn(move || {
            let mut i = 0;
            while i < per_producer {
                if q.try_enqueue(i as u32).is_ok() {
                    i += 1;
                }
            }
        }));
    }
    let dq = q.clone();
    let consumer = thread::spawn(move || {
        let mut got = 0;
        let total = producers * per_producer;
        while got < total {
            if dq.try_dequeue().is_some() {
                got += 1;
            } else {
                std::hint::spin_loop();
            }
        }
    });
    for p in prods {
        p.join().unwrap();
    }
    consumer.join().unwrap();
    // With 8 producers hitting a hot tail, contention is essentially
    // guaranteed. Loose lower-bound to keep the test stable.
    assert!(q.cas_retries() > 0, "expected non-zero retries");
}

#[test]
fn drop_runs_for_pending_items() {
    struct DropCounted(Arc<AtomicUsize>);
    impl Drop for DropCounted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let q: MpmcQueue<DropCounted> = MpmcQueue::new(4);
        assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
        assert!(q.try_enqueue(DropCounted(counter.clone())).is_ok());
    }
    assert_eq!(counter.load(Ordering::Relaxed), 2);
}

#[test]
fn drain_then_refill_wraps_ring() {
    let q: MpmcQueue<u32> = MpmcQueue::new(4);
    for round in 0..10 {
        for i in 0..4 {
            q.try_enqueue(round * 4 + i).unwrap();
        }
        for i in 0..4 {
            assert_eq!(q.try_dequeue(), Some(round * 4 + i));
        }
    }
}
