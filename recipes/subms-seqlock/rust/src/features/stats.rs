//! A reader that counts.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::seqlock::{SeqLockImage, SeqLockReader};

/// Shared counters.
#[derive(Default)]
pub struct ReadStats {
    reads: AtomicU64,
    retries: AtomicU64,
    max_retries: AtomicU64,
}

/// A point-in-time copy of the counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadStatsSnapshot {
    /// Successful reads.
    pub reads: u64,
    /// Attempts that were torn or saw a write in flight, summed.
    pub retries: u64,
    /// The most retries any single read needed.
    pub max_retries: u64,
}

impl ReadStatsSnapshot {
    /// Retries per successful read.
    pub fn retry_rate(&self) -> f64 {
        if self.reads == 0 {
            0.0
        } else {
            self.retries as f64 / self.reads as f64
        }
    }
}

impl ReadStats {
    pub fn snapshot(&self) -> ReadStatsSnapshot {
        ReadStatsSnapshot {
            reads: self.reads.load(Ordering::Relaxed),
            retries: self.retries.load(Ordering::Relaxed),
            max_retries: self.max_retries.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.reads.store(0, Ordering::Relaxed);
        self.retries.store(0, Ordering::Relaxed);
        self.max_retries.store(0, Ordering::Relaxed);
    }

    #[inline]
    fn record(&self, retries: u64) {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if retries > 0 {
            self.retries.fetch_add(retries, Ordering::Relaxed);
            self.max_retries.fetch_max(retries, Ordering::Relaxed);
        }
    }
}

/// A [`SeqLockReader`] that records what every read cost.
pub struct InstrumentedReader<T: SeqLockImage> {
    reader: SeqLockReader<T>,
    stats: Arc<ReadStats>,
}

impl<T: SeqLockImage> Clone for InstrumentedReader<T> {
    fn clone(&self) -> Self {
        InstrumentedReader {
            reader: self.reader.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl<T: SeqLockImage> InstrumentedReader<T> {
    /// Wrap a reader with a fresh counter set.
    pub fn new(reader: SeqLockReader<T>) -> InstrumentedReader<T> {
        InstrumentedReader {
            reader,
            stats: Arc::new(ReadStats::default()),
        }
    }

    /// Wrap a reader onto an existing counter set, so two locks can report into one place.
    pub fn with_stats(reader: SeqLockReader<T>, stats: Arc<ReadStats>) -> InstrumentedReader<T> {
        InstrumentedReader { reader, stats }
    }

    /// A consistent snapshot, with the attempt count recorded.
    #[inline]
    pub fn read(&self) -> T {
        let mut retries = 0u64;
        loop {
            if let Some((v, _)) = self.reader.try_read_once() {
                self.stats.record(retries);
                return v;
            }
            retries += 1;
            std::hint::spin_loop();
        }
    }

    #[inline]
    pub fn read_versioned(&self) -> (T, u64) {
        let mut retries = 0u64;
        loop {
            if let Some(r) = self.reader.try_read_once() {
                self.stats.record(retries);
                return r;
            }
            retries += 1;
            std::hint::spin_loop();
        }
    }

    /// At most `max_attempts` tries.
    #[inline]
    pub fn try_read(&self, max_attempts: usize) -> Option<T> {
        let mut retries = 0u64;
        for _ in 0..max_attempts {
            if let Some((v, _)) = self.reader.try_read_once() {
                self.stats.record(retries);
                return Some(v);
            }
            retries += 1;
            std::hint::spin_loop();
        }
        if retries > 0 {
            self.stats.retries.fetch_add(retries, Ordering::Relaxed);
            self.stats.max_retries.fetch_max(retries, Ordering::Relaxed);
        }
        None
    }

    pub fn stats(&self) -> &Arc<ReadStats> {
        &self.stats
    }

    pub fn snapshot(&self) -> ReadStatsSnapshot {
        self.stats.snapshot()
    }

    pub fn inner(&self) -> &SeqLockReader<T> {
        &self.reader
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
