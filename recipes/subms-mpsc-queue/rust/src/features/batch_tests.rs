use super::*;
use std::sync::Arc;
use std::thread;

fn collect_n<T>(out: &mut [Option<T>], n: usize) -> Vec<T> {
    out.iter_mut().take(n).map(|s| s.take().unwrap()).collect()
}

#[test]
fn batch_drains_up_to_buffer_size() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    for i in 0..10 {
        q.push(i);
    }
    let mut buf: Vec<Option<u32>> = (0..4).map(|_| None).collect();
    let n = q.try_dequeue_batch(&mut buf);
    assert_eq!(n, 4);
    assert_eq!(collect_n(&mut buf, n), vec![0, 1, 2, 3]);
}

#[test]
fn batch_stops_at_empty() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    q.push(1);
    q.push(2);
    let mut buf: Vec<Option<u32>> = (0..10).map(|_| None).collect();
    let n = q.try_dequeue_batch(&mut buf);
    assert_eq!(n, 2);
    assert_eq!(buf[0], Some(1));
    assert_eq!(buf[1], Some(2));
    // Re-call when empty returns zero.
    let n2 = q.try_dequeue_batch(&mut buf);
    assert_eq!(n2, 0);
}

#[test]
fn batch_preserves_fifo_order() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    for i in 0..100 {
        q.push(i);
    }
    let mut buf: Vec<Option<u32>> = (0..100).map(|_| None).collect();
    let n = q.try_dequeue_batch(&mut buf);
    assert_eq!(n, 100);
    for (i, slot) in buf.iter().enumerate().take(100) {
        assert_eq!(*slot, Some(i as u32));
    }
}

#[test]
fn drain_into_vec_works() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    for i in 0..50 {
        q.push(i);
    }
    let mut out = Vec::with_capacity(50);
    let n = q.drain_into_vec(&mut out, 50);
    assert_eq!(n, 50);
    for (i, v) in out.iter().enumerate() {
        assert_eq!(*v, i as u32);
    }
}

#[test]
fn drain_into_vec_stops_at_empty_below_cap() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    for i in 0..3u32 {
        q.push(i);
    }
    let mut out = Vec::new();
    // cap 10 but only 3 items available -> the loop exits via the
    // empty-branch break, not the cap bound.
    let n = q.drain_into_vec(&mut out, 10);
    assert_eq!(n, 3);
    assert_eq!(out, vec![0, 1, 2]);
}

#[test]
fn try_dequeue_batch_stops_at_empty_below_len() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    q.push(9);
    let mut buf: Vec<Option<u32>> = (0..8).map(|_| None).collect();
    // One item, eight slots -> break on the empty branch after draining one.
    let n = q.try_dequeue_batch(&mut buf);
    assert_eq!(n, 1);
    assert_eq!(buf[0], Some(9));
}

#[test]
fn multi_producer_batch_drain_loses_nothing() {
    let producers = 4usize;
    let per_producer = 10_000usize;
    let q: Arc<BatchMpscQueue<u64>> = Arc::new(BatchMpscQueue::new());
    let mut prods = Vec::new();
    for tid in 0..producers as u64 {
        let q = q.clone();
        prods.push(thread::spawn(move || {
            for i in 0..per_producer as u64 {
                q.push((tid << 32) | i);
            }
        }));
    }
    let cq = q.clone();
    let consumer = thread::spawn(move || {
        let qp = Arc::as_ptr(&cq) as *mut BatchMpscQueue<u64>;
        let qm = unsafe { &mut *qp };
        let mut counts = [0u64; 4];
        let target = producers * per_producer;
        let mut total = 0usize;
        let mut buf: Vec<Option<u64>> = (0..256).map(|_| None).collect();
        while total < target {
            let n = qm.try_dequeue_batch(&mut buf);
            for slot in buf.iter_mut().take(n) {
                let v = slot.take().unwrap();
                counts[(v >> 32) as usize] += 1;
                total += 1;
            }
            if n == 0 {
                std::hint::spin_loop();
            }
        }
        counts
    });
    for p in prods {
        p.join().unwrap();
    }
    let counts = consumer.join().unwrap();
    for c in counts {
        assert_eq!(c as usize, per_producer);
    }
}

#[test]
fn empty_buffer_returns_zero() {
    let mut q: BatchMpscQueue<u32> = BatchMpscQueue::new();
    q.push(1);
    let mut buf: Vec<Option<u32>> = Vec::new();
    let n = q.try_dequeue_batch(&mut buf);
    assert_eq!(n, 0);
}

#[test]
fn default_constructor_works() {
    let q: BatchMpscQueue<u32> = BatchMpscQueue::default();
    q.push(1);
    let mut qb = q;
    let mut buf: Vec<Option<u32>> = (0..4).map(|_| None).collect();
    assert_eq!(qb.try_dequeue_batch(&mut buf), 1);
}
