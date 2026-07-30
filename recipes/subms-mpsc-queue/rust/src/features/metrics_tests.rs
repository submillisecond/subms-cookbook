use super::*;

fn pop_until_some<T>(q: &mut MetricsMpscQueue<T>) -> Option<T> {
    let start = std::time::Instant::now();
    loop {
        match q.try_pop() {
            PopResult::Some(v) => return Some(v),
            PopResult::Empty => return None,
            PopResult::Inconsistent => {
                if start.elapsed().as_secs() > 5 {
                    return None;
                }
                std::hint::spin_loop();
            }
        }
    }
}

#[test]
fn pop_until_some_returns_none_on_empty_queue() {
    // Drives the Empty arm of the helper: an empty queue yields None
    // immediately rather than spinning.
    let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    assert_eq!(pop_until_some(&mut q), None);
}

#[test]
fn snapshot_default_is_zero() {
    let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
}

#[test]
fn push_increments_enqueue_ok() {
    let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    for i in 0..5 {
        q.push(i);
    }
    let s = q.snapshot();
    assert_eq!(s.enqueue_ok, 5);
    assert_eq!(s.enqueue_fail, 0);
}

#[test]
fn try_pop_tracks_ok_and_fail() {
    let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    q.push(1);
    q.push(2);
    let _ = pop_until_some(&mut q).unwrap();
    let _ = pop_until_some(&mut q).unwrap();
    // Now empty:
    let _ = q.try_pop(); // empty
    let s = q.snapshot();
    assert_eq!(s.dequeue_ok, 2);
    assert!(s.dequeue_fail >= 1);
}

#[test]
fn batch_records_drained_items() {
    let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    for i in 0..7 {
        q.push(i);
    }
    let mut buf: Vec<Option<u32>> = (0..10).map(|_| None).collect();
    let n = q.try_pop_batch(&mut buf);
    let s = q.snapshot();
    assert_eq!(n, 7);
    assert_eq!(s.batch_items, 7);
    assert_eq!(s.dequeue_ok, 7);
}

#[test]
fn record_enqueue_fail_bumps_counter() {
    let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    q.push(1);
    q.record_enqueue_fail();
    q.record_enqueue_fail();
    let s = q.snapshot();
    assert_eq!(s.enqueue_ok, 1);
    assert_eq!(s.enqueue_fail, 2);
}

#[test]
fn record_cas_retries_accumulates() {
    let q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    q.record_cas_retries(5);
    q.record_cas_retries(3);
    q.record_cas_retries(0); // explicit no-op
    let s = q.snapshot();
    assert_eq!(s.cas_retries, 8);
}

#[test]
fn reset_clears_all_counters() {
    let mut q: MetricsMpscQueue<u32> = MetricsMpscQueue::new();
    q.push(1);
    q.push(2);
    let _ = pop_until_some(&mut q);
    q.record_cas_retries(7);
    q.record_enqueue_fail();
    q.reset();
    assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
}

#[test]
fn default_constructor_works() {
    let q: MetricsMpscQueue<u32> = MetricsMpscQueue::default();
    assert_eq!(q.snapshot(), QueueMetricsSnapshot::default());
}
