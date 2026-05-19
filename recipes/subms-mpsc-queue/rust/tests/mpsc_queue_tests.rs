use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use subms_mpsc_queue::{MpscQueue, PopResult};

fn pop_eventually<T>(q: &mut MpscQueue<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        match q.try_pop() {
            PopResult::Some(v) => return Some(v),
            PopResult::Inconsistent => {
                if start.elapsed().as_secs() > 5 {
                    panic!("stuck in Inconsistent");
                }
                std::hint::spin_loop();
            }
            PopResult::Empty => return None,
        }
    }
}

#[test]
fn push_and_pop_a_single_value() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    q.push(7);
    assert_eq!(pop_eventually(&mut q), Some(7));
    assert_eq!(pop_eventually(&mut q), None);
}

#[test]
fn empty_pop_returns_empty() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert!(matches!(q.try_pop(), PopResult::Empty));
    assert!(matches!(q.try_pop(), PopResult::Empty));
}

#[test]
fn fifo_order_single_producer() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    for i in 0..100u32 {
        q.push(i);
    }
    for i in 0..100u32 {
        assert_eq!(pop_eventually(&mut q), Some(i));
    }
    assert_eq!(pop_eventually(&mut q), None);
}

#[test]
fn alternating_push_pop() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    for i in 0..1000u32 {
        q.push(i);
        assert_eq!(pop_eventually(&mut q), Some(i));
    }
}

#[test]
fn drain_then_refill() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    for i in 0..50u32 {
        q.push(i);
    }
    for i in 0..50u32 {
        assert_eq!(pop_eventually(&mut q), Some(i));
    }
    assert_eq!(pop_eventually(&mut q), None);
    for i in 100..120u32 {
        q.push(i);
    }
    for i in 100..120u32 {
        assert_eq!(pop_eventually(&mut q), Some(i));
    }
}

#[test]
fn multi_producer_no_lost_items() {
    let producers = 4usize;
    let per_producer = 100_000usize;
    let q: Arc<MpscQueue<u64>> = Arc::new(MpscQueue::new());
    let mut handles = Vec::new();
    for tid in 0..producers as u64 {
        let q = q.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_producer as u64 {
                q.push((tid << 32) | i);
            }
        }));
    }
    let consumer_q = q.clone();
    let consumer = thread::spawn(move || {
        let q_ptr = Arc::as_ptr(&consumer_q) as *mut MpscQueue<u64>;
        let q_mut = unsafe { &mut *q_ptr };
        let mut counts = [0usize; 4];
        let mut total = 0usize;
        while total < producers * per_producer {
            match q_mut.try_pop() {
                PopResult::Some(v) => {
                    let tid = (v >> 32) as usize;
                    counts[tid] += 1;
                    total += 1;
                }
                _ => std::hint::spin_loop(),
            }
        }
        counts
    });
    for h in handles {
        h.join().unwrap();
    }
    let counts = consumer.join().unwrap();
    for c in counts {
        assert_eq!(c, per_producer);
    }
}

#[test]
fn higher_producer_contention() {
    let producers = 8usize;
    let per_producer = 25_000usize;
    let q: Arc<MpscQueue<u64>> = Arc::new(MpscQueue::new());
    let mut handles = Vec::new();
    for tid in 0..producers as u64 {
        let q = q.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_producer as u64 {
                q.push((tid << 32) | i);
            }
        }));
    }
    let consumer_q = q.clone();
    let consumer = thread::spawn(move || {
        let q_ptr = Arc::as_ptr(&consumer_q) as *mut MpscQueue<u64>;
        let q_mut = unsafe { &mut *q_ptr };
        let mut total = 0usize;
        while total < producers * per_producer {
            if let PopResult::Some(_) = q_mut.try_pop() {
                total += 1;
            }
        }
        total
    });
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(consumer.join().unwrap(), producers * per_producer);
}

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
        let q: MpscQueue<DropCounted> = MpscQueue::new();
        q.push(DropCounted(counter.clone()));
        q.push(DropCounted(counter.clone()));
        q.push(DropCounted(counter.clone()));
    }
    assert_eq!(counter.load(Ordering::Relaxed), 3);
}

#[test]
fn popped_items_drop_only_once() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut q: MpscQueue<DropCounted> = MpscQueue::new();
    q.push(DropCounted(counter.clone()));
    let v = pop_eventually(&mut q).unwrap();
    assert_eq!(
        counter.load(Ordering::Relaxed),
        0,
        "not dropped while owned"
    );
    drop(v);
    assert_eq!(
        counter.load(Ordering::Relaxed),
        1,
        "dropped exactly once after release"
    );
}

#[test]
fn large_single_thread_workload() {
    let mut q: MpscQueue<u64> = MpscQueue::new();
    let n = 100_000u64;
    for i in 0..n {
        q.push(i);
    }
    let mut next = 0u64;
    loop {
        match q.try_pop() {
            PopResult::Some(v) => {
                assert_eq!(v, next);
                next += 1;
            }
            PopResult::Inconsistent => continue,
            PopResult::Empty => break,
        }
    }
    assert_eq!(next, n);
}

#[test]
fn default_constructor_works() {
    let mut q: MpscQueue<u32> = MpscQueue::default();
    q.push(1);
    assert_eq!(pop_eventually(&mut q), Some(1));
}
