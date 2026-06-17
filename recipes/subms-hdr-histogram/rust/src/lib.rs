//! Log-linear bucket histogram with significant-digit precision.
//!
//! Each value is mapped to a bucket index built from two pieces:
//!
//! - **Major bucket**: `floor(log2(value)) - log2(sub_count) + 1`, clamped at 0.
//!   Each major covers a doubling range (1, 2, 4, 8, ...).
//! - **Sub-bucket**: linear position within the major range.
//!
//! Together they give constant relative error within the significant-digit
//! precision. A 3-sig-digit histogram has `2^9 = 512` sub-buckets and covers
//! 1 .. ~10^9 in ~33 major buckets, ~17k counters total.
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
    /// `significant_digits` ∈ [1, 5]; clamped if out of range.
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

    /// Value at the given quantile (`0.0..=1.0`). 0 if empty.
    pub fn value_at_percentile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = ((q.clamp(0.0, 1.0) * self.total as f64) as u64).max(1);
        let mut cum = 0u64;
        for (i, &c) in self.counters.iter().enumerate() {
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
