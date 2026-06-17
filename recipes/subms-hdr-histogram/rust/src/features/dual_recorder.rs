//! Recorder pattern: two histograms alternated for lock-free
//! producer / occasional consumer.
//!
//! Producers call `record()` on the recorder, which forwards to the
//! currently-active concurrent histogram. The consumer calls
//! `get_interval_histogram()`, which atomically swaps the active
//! and inactive sides and drains the newly-inactive one. The
//! producer's hot path never blocks - it always sees one valid
//! histogram, accessible without locks.
//!
//! Pattern taken from HdrHistogram-Java's `Recorder`. Useful for
//! interval reporting loops: every N seconds the consumer grabs an
//! interval snapshot without disturbing producers.

use crate::features::concurrent_writes::{ConcurrentHdrHistogram, Snapshot};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Two-buffer concurrent histogram. Producers hit the active side;
/// the consumer rotates and drains the inactive side.
pub struct DualRecorder {
    histograms: [ConcurrentHdrHistogram; 2],
    /// Index of the currently-active histogram (0 or 1).
    active: AtomicUsize,
}

impl DualRecorder {
    /// New recorder with the given significant-digit precision.
    /// Both internal histograms share the same shape.
    pub fn new(significant_digits: u32) -> Self {
        Self::with_majors(significant_digits, 32)
    }

    /// Explicit major-bucket capacity (passed to each inner
    /// `ConcurrentHdrHistogram`). Both sides use the same shape so
    /// snapshots from one are interchangeable with the other.
    pub fn with_majors(significant_digits: u32, majors: u32) -> Self {
        Self {
            histograms: [
                ConcurrentHdrHistogram::with_majors(significant_digits, majors),
                ConcurrentHdrHistogram::with_majors(significant_digits, majors),
            ],
            active: AtomicUsize::new(0),
        }
    }

    /// Record a value into the currently-active histogram.
    /// Lock-free; safe from any thread.
    pub fn record(&self, value: u64) {
        let idx = self.active.load(Ordering::Acquire);
        self.histograms[idx].record(value);
    }

    /// Atomically rotate the active side and drain the newly-inactive
    /// side. Producers that race the rotation may land their write
    /// on EITHER side - both are valid live targets at that moment.
    /// The returned snapshot reflects all records that landed on the
    /// outgoing side before the rotation completed.
    ///
    /// Only one consumer thread should call this at a time. The two
    /// histograms are not designed for multiple drainers per cycle.
    pub fn get_interval_histogram(&self) -> Snapshot {
        let prev = self.active.load(Ordering::Acquire);
        let next = 1 - prev;
        // Flip first so any subsequent record() lands on `next`.
        self.active.store(next, Ordering::Release);
        // Drain the now-inactive side. Concurrent producers that
        // already started a record() on `prev` get counted toward
        // this snapshot, which is the conservative choice.
        self.histograms[prev].drain_snapshot()
    }

    /// Index of the currently-active histogram. Exposed for tests
    /// and observability; production callers shouldn't need it.
    pub fn active_index(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
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
}
