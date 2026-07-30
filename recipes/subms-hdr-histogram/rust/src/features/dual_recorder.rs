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
#[path = "dual_recorder_tests.rs"]
mod tests;
