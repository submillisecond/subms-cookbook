//! Log-linear bucket histogram with significant-digit precision.
//!
//! Each value is mapped to a bucket index built from two pieces:
//!
//! - **Major bucket**: `floor(log2(value)) - log2(sub_count) + 1`, clamped at 0.
//!   Each major covers a doubling range (1, 2, 4, 8, ...).
//! - **Sub-bucket**: linear position within the major range.
//!
//! Together they give constant relative error within the significant-digit
//! precision: a value's bucket is never wider than `1 / sub_count` of the value
//! itself. `sub_count` is `2 * 10^d` rounded up to a power of two, so `d = 3`
//! gives `2^11 = 2048` sub-buckets and a worst-case quantisation error of
//! 1/2048 (0.049%), inside the half-unit-in-the-third-digit that three
//! significant digits demands.
//!
//! The counter array starts at `sub_count` entries and grows lazily to cover
//! the largest value recorded: at `d = 3` a range topping out at 10^6 lands at
//! index 20290 (~20k counters, 163 KB), and one topping out at 10^9 at index
//! 40678 (~41k counters, 326 KB).
//!
//! ```
//! use subms_hdr_histogram::HdrHistogram;
//! let mut h = HdrHistogram::new(3);
//! for v in [10u64, 20, 30, 40, 50] { h.record(v); }
//! assert_eq!(h.count(), 5);
//! let p50 = h.value_at_percentile(0.5);
//! assert!((20..=30).contains(&p50), "p50={p50}");
//! assert_eq!(h.max(), 50);
//! ```
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-hdr-histogram>

/// Histogram with `significant_digits` of precision in `[1, 5]`.
pub struct HdrHistogram {
    /// Number of sub-buckets within each major bucket. Power of two.
    sub_count: u32,
    /// Bit-width of sub_count (so `value >> shift` ignores the sub portion).
    sub_count_bits: u32,
    /// Flat counter array; length grows as bigger values are recorded.
    counters: Vec<u64>,
    /// Total count across counters.
    total: u64,
    /// Highest non-zero counter index seen, for fast iteration.
    high_index: usize,
}

impl HdrHistogram {
    /// `significant_digits` in `[1, 5]`; clamped if out of range.
    pub fn new(significant_digits: u32) -> Self {
        let sig = significant_digits.clamp(1, 5);
        // sub_count = 2 * 10^sig rounded up to next power of two.
        let target = 2u32 * 10u32.pow(sig);
        let sub_count_bits = (32 - target.leading_zeros()).max(1);
        let sub_count = 1u32 << sub_count_bits;
        Self {
            sub_count,
            sub_count_bits,
            counters: vec![0u64; sub_count as usize],
            total: 0,
            high_index: 0,
        }
    }

    pub fn count(&self) -> u64 {
        self.total
    }

    /// Highest value recorded (approximated to the bucket's lower bound).
    pub fn max(&self) -> u64 {
        if self.total == 0 {
            return 0;
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    pub fn record(&mut self, value: u64) {
        let idx = index_of(value, self.sub_count_bits) as usize;
        if idx >= self.counters.len() {
            self.counters.resize(idx + 1, 0);
        }
        self.counters[idx] += 1;
        self.total += 1;
        if idx > self.high_index {
            self.high_index = idx;
        }
    }

    /// Record `value`, then correct for coordinated omission. Under a fixed-rate
    /// load generator, one slow operation blocks every request that should have
    /// been issued while it stalled; those requests are never sampled, so the
    /// tail reads far better than the system delivered. When `value` exceeds
    /// `expected_interval`, this backfills the samples the generator would have
    /// taken during the stall - synthetic values at `value - expected_interval`,
    /// `value - 2*expected_interval`, ... down to `expected_interval` - so the
    /// percentiles reflect the latency those blocked requests would have seen.
    ///
    /// This is Gil Tene's `recordValueWithExpectedInterval`. `expected_interval
    /// == 0` (or a `value` no larger than it) disables the correction, leaving
    /// this equivalent to [`Self::record`].
    pub fn record_with_expected_interval(&mut self, value: u64, expected_interval: u64) {
        self.record(value);
        // Guard the u64 subtraction below: also the correct no-op when the op
        // ran at or under the expected cadence (nothing was omitted).
        if expected_interval == 0 || value <= expected_interval {
            return;
        }
        let mut missing = value - expected_interval;
        while missing >= expected_interval {
            self.record(missing);
            missing -= expected_interval;
        }
    }

    /// Value at the given quantile (`0.0..=1.0`). 0 if empty.
    pub fn value_at_percentile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * self.total as f64) as u64).max(1);
        let mut cum = 0u64;
        // Bound the sweep to the populated range (parity with the Java port); the
        // dead tail past high_index holds only zeros and never shifts the result.
        for (i, &c) in self.counters.iter().take(self.high_index + 1).enumerate() {
            cum += c;
            if cum >= target {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        value_from_index(self.high_index, self.sub_count_bits)
    }

    pub fn sub_count(&self) -> u32 {
        self.sub_count
    }

    /// Lowest value recorded, as its bucket's lower bound. 0 if empty.
    /// Read-side sweep, same cost class as [`Self::value_at_percentile`].
    pub fn min(&self) -> u64 {
        if self.total == 0 {
            return 0;
        }
        for (i, &c) in self.counters.iter().take(self.high_index + 1).enumerate() {
            if c > 0 {
                return value_from_index(i, self.sub_count_bits);
            }
        }
        0
    }

    /// Arithmetic mean over the recorded bucket lower bounds. 0.0 if empty.
    /// Quantised the same way the percentiles are, so it sits within the
    /// significant-digit error band rather than being exact.
    pub fn mean(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let mut sum = 0f64;
        for (i, &c) in self.counters.iter().take(self.high_index + 1).enumerate() {
            if c > 0 {
                sum += c as f64 * value_from_index(i, self.sub_count_bits) as f64;
            }
        }
        sum / self.total as f64
    }

    /// Recordings that landed in `value`'s bucket. Constant time - the same
    /// index computation `record` does.
    pub fn count_at_value(&self, value: u64) -> u64 {
        let idx = index_of(value, self.sub_count_bits) as usize;
        self.counters.get(idx).copied().unwrap_or(0)
    }

    /// Fraction of recordings at or below `value`'s bucket, in `0.0..=1.0`.
    /// The inverse of [`Self::value_at_percentile`]: that maps a rank to a
    /// value, this maps a value to its rank.
    pub fn percentile_at_or_below_value(&self, value: u64) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let idx = index_of(value, self.sub_count_bits) as usize;
        let end = (idx + 1).min(self.high_index + 1).min(self.counters.len());
        let cum: u64 = self.counters[..end].iter().sum();
        cum as f64 / self.total as f64
    }

    /// Counter-array footprint in bytes. Grows with the largest value
    /// recorded, not with how many values were recorded.
    pub fn footprint_bytes(&self) -> usize {
        self.counters.len() * size_of::<u64>()
    }

    /// Drop every recorded value. Keeps the array allocated, so a histogram
    /// recycled across reporting intervals never re-enters the allocator.
    pub fn reset(&mut self) {
        self.counters.fill(0);
        self.total = 0;
        self.high_index = 0;
    }

    // ----- crate-private accessors for feature modules -----

    #[cfg(feature = "iterators")]
    #[inline]
    pub(crate) fn sub_count_bits(&self) -> u32 {
        self.sub_count_bits
    }

    #[cfg(feature = "iterators")]
    #[inline]
    pub(crate) fn counters(&self) -> &[u64] {
        &self.counters
    }

    #[cfg(feature = "iterators")]
    #[inline]
    pub(crate) fn high_index(&self) -> usize {
        self.high_index
    }

    /// Add another histogram's counters into this one. Used by the
    /// `merge` feature module. Errors if the two histograms have
    /// different `sub_count_bits` (different significant-digit shapes).
    #[cfg(feature = "merge")]
    pub(crate) fn add_counts_from(&mut self, other: &HdrHistogram) -> Result<(), &'static str> {
        if self.sub_count_bits != other.sub_count_bits {
            return Err("significant-digit mismatch");
        }
        if other.high_index >= self.counters.len() {
            self.counters.resize(other.high_index + 1, 0);
        }
        for (i, &c) in other.counters.iter().enumerate() {
            if c == 0 {
                continue;
            }
            self.counters[i] += c;
            if i > self.high_index {
                self.high_index = i;
            }
        }
        self.total += other.total;
        Ok(())
    }
}

/// Bucket index. Values `< sub_count` go in the linear part of the first
/// major. Larger values use a major bucket equal to `bits(value) - bits(sub_count-1)`.
pub(crate) fn index_of(value: u64, sub_count_bits: u32) -> u32 {
    let sub_mask = (1u64 << sub_count_bits) - 1;
    if value <= sub_mask {
        return value as u32;
    }
    let bits = 64 - value.leading_zeros();
    let major = bits - sub_count_bits;
    // sub portion: top sub_count_bits bits of value after the leading 1.
    let sub = ((value >> (major - 1)) & sub_mask) as u32;
    (major << sub_count_bits) | sub
}

pub(crate) fn value_from_index(idx: usize, sub_count_bits: u32) -> u64 {
    let sub_count = 1u64 << sub_count_bits;
    let sub_mask = sub_count - 1;
    let idx = idx as u64;
    if idx < sub_count {
        return idx;
    }
    let major = idx >> sub_count_bits;
    let sub = idx & sub_mask;
    (sub | sub_count) << (major - 1)
}

#[cfg(test)]
#[path = "hdr_tests.rs"]
mod hdr_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Base histogram is zero-dep + std-only; each
// opt-in adds a focused capability under its own Cargo feature.
#[cfg(any(
    feature = "dual-recorder",
    feature = "concurrent-writes",
    feature = "merge",
    feature = "decay",
    feature = "value-tagging",
    feature = "iterators",
))]
pub mod features;

#[cfg(feature = "concurrent-writes")]
pub use features::concurrent_writes::ConcurrentHdrHistogram;
#[cfg(feature = "decay")]
pub use features::decay::{Clock, DecayingHdrHistogram, ManualClock};
#[cfg(feature = "dual-recorder")]
pub use features::dual_recorder::DualRecorder;
#[cfg(feature = "iterators")]
pub use features::iterators::{HdrLinearIter, HdrLogarithmicIter, HdrPercentileIter, IterEntry};
#[cfg(feature = "merge")]
pub use features::merge::merge;
#[cfg(feature = "value-tagging")]
pub use features::value_tagging::TaggedHdrHistogram;
