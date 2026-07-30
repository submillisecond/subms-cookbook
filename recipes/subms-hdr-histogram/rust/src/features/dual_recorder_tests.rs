use super::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

#[test]
fn empty_drain_is_zero() {
    let rec = DualRecorder::new(3);
    let snap = rec.get_interval_histogram();
    assert_eq!(snap.count(), 0);
    assert_eq!(snap.value_at_percentile(0.99), 0);
}

#[test]
fn drain_returns_records_since_last_rotate() {
    let rec = DualRecorder::new(3);
    for i in 1..=100 {
        rec.record(i);
    }
    let snap = rec.get_interval_histogram();
    assert_eq!(snap.count(), 100);
    // Next drain should be empty - the active side rotated.
    let empty = rec.get_interval_histogram();
    assert_eq!(empty.count(), 0);
}

#[test]
fn rotation_swaps_active_side() {
    let rec = DualRecorder::new(3);
    let first = rec.active_index();
    rec.record(10);
    let _ = rec.get_interval_histogram();
    let second = rec.active_index();
    assert_ne!(first, second, "active side flipped");
}

#[test]
fn records_after_rotate_go_to_new_side() {
    let rec = DualRecorder::new(3);
    for i in 1..=50 {
        rec.record(i);
    }
    let first = rec.get_interval_histogram();
    for i in 1..=10 {
        rec.record(i * 100);
    }
    let second = rec.get_interval_histogram();
    assert_eq!(first.count(), 50);
    assert_eq!(second.count(), 10);
    assert!(second.max() >= 1000);
}

#[test]
fn concurrent_writers_with_periodic_drain() {
    let rec = Arc::new(DualRecorder::new(3));
    let stop = Arc::new(AtomicBool::new(false));
    let producers = 6;
    let per_producer = 20_000;

    let mut handles = vec![];
    for t in 0..producers {
        let rec = rec.clone();
        let stop = stop.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0u64;
            while !stop.load(Ordering::Acquire) && i < per_producer {
                rec.record(((t as u64 * per_producer) + i) % 1000 + 1);
                i += 1;
            }
            i
        }));
    }

    // Periodic drainer accumulates samples across rotations.
    let drainer_rec = rec.clone();
    let drainer_stop = stop.clone();
    let drainer = thread::spawn(move || {
        let mut total = 0u64;
        while !drainer_stop.load(Ordering::Acquire) {
            let s = drainer_rec.get_interval_histogram();
            total += s.count();
            // Yield to producers; busy-spin would starve them.
            thread::yield_now();
        }
        // Final drain to pick up trailing writes.
        let s = drainer_rec.get_interval_histogram();
        total += s.count();
        total
    });

    // Let producers run to completion.
    let mut produced = 0u64;
    for h in handles {
        produced += h.join().unwrap();
    }
    stop.store(true, Ordering::Release);
    let drained = drainer.join().unwrap();
    assert_eq!(
        drained, produced,
        "every record() must show up in some snapshot: produced={produced} drained={drained}"
    );
}

#[test]
fn back_to_back_drain_alternates_sides() {
    let rec = DualRecorder::new(3);
    let i0 = rec.active_index();
    let _ = rec.get_interval_histogram();
    let i1 = rec.active_index();
    let _ = rec.get_interval_histogram();
    let i2 = rec.active_index();
    assert_ne!(i0, i1);
    assert_eq!(i0, i2);
}
