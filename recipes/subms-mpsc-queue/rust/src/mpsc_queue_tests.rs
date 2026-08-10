use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use super::*;

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

#[test]
fn peek_borrows_without_consuming() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert!(q.peek().is_none(), "nothing to peek on a fresh queue");
    q.push(11);
    q.push(22);
    assert_eq!(q.peek(), Some(&11));
    assert_eq!(q.peek(), Some(&11), "peek is idempotent");
    assert_eq!(pop_eventually(&mut q), Some(11));
    assert_eq!(q.peek(), Some(&22), "peek follows the consumer forward");
    assert_eq!(pop_eventually(&mut q), Some(22));
    assert!(q.peek().is_none());
}

#[test]
fn is_empty_tracks_the_drain() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert!(q.is_empty());
    q.push(1);
    assert!(!q.is_empty());
    assert_eq!(pop_eventually(&mut q), Some(1));
    assert!(q.is_empty());
}

#[test]
fn len_counts_the_backlog() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert_eq!(q.len(), 0);
    for i in 0..5 {
        q.push(i);
    }
    assert_eq!(q.len(), 5);
    assert_eq!(pop_eventually(&mut q), Some(0));
    assert_eq!(q.len(), 4, "len tracks the consumer's position");
}

#[test]
fn clear_drains_and_reports_the_count() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert_eq!(q.clear(), 0, "clearing an empty queue is a no-op");
    for i in 0..7 {
        q.push(i);
    }
    assert_eq!(q.clear(), 7);
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);
    q.push(99);
    assert_eq!(
        pop_eventually(&mut q),
        Some(99),
        "the queue is reusable after a clear"
    );
}

#[test]
fn clear_runs_the_dropped_items_destructors() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut q: MpscQueue<DropCounted> = MpscQueue::new();
    for _ in 0..4 {
        q.push(DropCounted(counter.clone()));
    }
    assert_eq!(q.clear(), 4);
    assert_eq!(counter.load(Ordering::Relaxed), 4);
}

#[cfg(feature = "batch")]
#[test]
fn push_batch_publishes_a_whole_run_in_order() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    assert_eq!(
        q.push_batch(Vec::<u32>::new()),
        0,
        "an empty run publishes nothing"
    );
    assert!(q.is_empty());

    assert_eq!(q.push_batch(vec![1, 2, 3]), 3);
    assert_eq!(q.len(), 3);
    for expected in 1..=3 {
        assert_eq!(pop_eventually(&mut q), Some(expected));
    }
    assert!(q.is_empty());
}

#[cfg(feature = "batch")]
#[test]
fn push_batch_interleaves_with_single_pushes() {
    let mut q: MpscQueue<u32> = MpscQueue::new();
    q.push(0);
    q.push_batch(1..=3);
    q.push(4);
    q.push_batch(std::iter::once(5));
    let mut seen = Vec::new();
    while let Some(v) = pop_eventually(&mut q) {
        seen.push(v);
    }
    assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
}

#[cfg(feature = "batch")]
#[test]
fn concurrent_push_batch_loses_nothing() {
    const PRODUCERS: u32 = 4;
    const RUNS: u32 = 250;
    const RUN_LEN: u32 = 8;

    let q: Arc<MpscQueue<u32>> = Arc::new(MpscQueue::new());
    let handles: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for r in 0..RUNS {
                    let base = p * RUNS * RUN_LEN + r * RUN_LEN;
                    q.push_batch(base..base + RUN_LEN);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let mut q = Arc::into_inner(q).expect("producers all joined");
    let total = (PRODUCERS * RUNS * RUN_LEN) as usize;
    let mut seen = vec![false; total];
    let mut drained = 0usize;
    while let Some(v) = pop_eventually(&mut q) {
        assert!(!seen[v as usize], "no item published twice");
        seen[v as usize] = true;
        drained += 1;
    }
    assert_eq!(drained, total, "every batched item arrives exactly once");
}
