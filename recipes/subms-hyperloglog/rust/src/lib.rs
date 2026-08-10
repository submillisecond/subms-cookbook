//! HyperLogLog cardinality estimator.
//!
//! Precision `p` in [4, 18]. The register array is `m = 2^p` bytes. Hashes
//! split into a `p`-bit register index (top bits) and a leading-zero count on
//! the remaining bits. Each register stores the max observed count + 1.
//! Estimate is the harmonic mean of `2^r` over the registers, scaled by an
//! `alpha_m` correction. Linear-counting kicks in at low cardinality where
//! the raw estimator is biased.
//!
//! ```
//! use subms_hyperloglog::HyperLogLog;
//! let mut hll = HyperLogLog::new(14);
//! for i in 0..10_000 { hll.add(&format!("key{i}")); }
//! let est = hll.estimate();
//! assert!(est > 9_000.0 && est < 11_000.0, "10k distinct within 10%, got {est}");
//! ```
//!
//! # Thread safety
//!
//! A `HyperLogLog` is a single-writer structure. `add`, `merge` and `clear`
//! take `&mut self`, so the compiler already stops two threads sharing one
//! sketch without a lock. The fan-in pattern is a sketch per thread or shard
//! and one `merge` at read time; the merge is exact, so nothing is lost by
//! never sharing a writer. `estimate` takes `&self` and is safe to call
//! concurrently on a sketch nobody is writing.
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-hyperloglog>

pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

/// Lowest precision the estimator is calibrated for.
pub const MIN_PRECISION: u32 = 4;
/// Highest precision this recipe allocates for. 2^18 registers is 256 KB.
pub const MAX_PRECISION: u32 = 18;

/// Flajolet's asymptotic relative standard error constant. Standard error is
/// `RSE_CONSTANT / sqrt(m)`.
pub const RSE_CONSTANT: f64 = 1.04;

mod codec;
mod error;
pub use codec::{FORMAT_VERSION, MAGIC};
pub use error::HllError;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq)]
pub struct HyperLogLog {
    p: u32,
    m: u32,
    pub(crate) registers: Vec<u8>,
    alpha: f64,
}

impl core::fmt::Debug for HyperLogLog {
    /// Deliberately does not dump the register array - at p=14 that is 16384
    /// bytes into whatever log caught the assertion.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HyperLogLog")
            .field("p", &self.p)
            .field("m", &self.m)
            .field("estimate", &self.estimate())
            .finish()
    }
}

impl HyperLogLog {
    /// New empty HLL at the given precision. `precision` is clamped to
    /// `[4, 18]`; 14 gives ~16k registers / ~16 KB / ~1% std error. Use
    /// [`HyperLogLog::try_new`] when a caller-supplied precision should be
    /// rejected rather than silently pulled into range.
    pub fn new(precision: u32) -> Self {
        let p = precision.clamp(MIN_PRECISION, MAX_PRECISION);
        let m = 1u32 << p;
        let alpha = alpha_m(m);
        Self {
            p,
            m,
            registers: vec![0u8; m as usize],
            alpha,
        }
    }

    /// New empty HLL, rejecting a precision outside `[4, 18]` instead of
    /// clamping it. Reach for this when the precision comes from config or a
    /// wire message and a typo should fail loudly.
    pub fn try_new(precision: u32) -> Result<Self, HllError> {
        if !(MIN_PRECISION..=MAX_PRECISION).contains(&precision) {
            return Err(HllError::InvalidPrecision(precision));
        }
        Ok(Self::new(precision))
    }

    pub fn precision(&self) -> u32 {
        self.p
    }
    pub fn register_count(&self) -> u32 {
        self.m
    }

    /// Analytic relative standard error, `1.04 / sqrt(m)`. This is the error
    /// the structure carries by construction, not a measurement of the current
    /// contents: at p=14 it is 0.813%, so a 1,000,000 estimate is one standard
    /// deviation away from anything in [992k, 1008k].
    pub fn standard_error(&self) -> f64 {
        RSE_CONSTANT / (self.m as f64).sqrt()
    }

    /// Smallest precision whose standard error is at or below `target`
    /// (expressed as a fraction, so 0.01 for 1%). Clamped to `[4, 18]`, so a
    /// target finer than 0.26% returns 18 and the caller gets the best this
    /// recipe allocates for rather than an error.
    pub fn precision_for_standard_error(target: f64) -> u32 {
        for p in MIN_PRECISION..MAX_PRECISION {
            let m = (1u32 << p) as f64;
            if RSE_CONSTANT / m.sqrt() <= target {
                return p;
            }
        }
        MAX_PRECISION
    }

    /// Bytes of register state this sketch holds. Fixed at construction and
    /// independent of how many items it has seen.
    pub fn state_bytes(&self) -> usize {
        self.registers.len()
    }

    /// True while every register is still zero.
    pub fn is_empty(&self) -> bool {
        self.registers.iter().all(|&r| r == 0)
    }

    /// Zero every register, keeping the allocation. Reuse across windows
    /// without re-allocating the array.
    pub fn clear(&mut self) {
        self.registers.fill(0);
    }

    /// Record a key. Returns true when the sketch changed - a register moved
    /// up, so this key was the first of its kind to land that deep. Matching
    /// `PFADD`'s return, and cheap enough to ignore when you do not want it.
    pub fn add(&mut self, key: &str) -> bool {
        self.add_bytes(key.as_bytes())
    }

    /// Record raw bytes. The string path funnels through here, so `add("AAPL")`
    /// and `add_bytes(b"AAPL")` land in the same register.
    pub fn add_bytes(&mut self, key: &[u8]) -> bool {
        self.add_hash(fnv1a64(key))
    }

    /// Record a 64-bit id without rendering it to a string first. Hashes the
    /// big-endian bytes, so the Rust and Java ports agree register for
    /// register on the same id.
    pub fn add_u64(&mut self, key: u64) -> bool {
        self.add_bytes(&key.to_be_bytes())
    }

    fn add_hash(&mut self, h: u64) -> bool {
        let idx = (h >> (64 - self.p)) as usize;
        // Use the remaining 64-p bits for the leading-zero count. Place a
        // sentinel 1 at the bottom so leading_zeros never exceeds (64-p).
        let w = (h << self.p) | (1u64 << (self.p - 1));
        let r = (w.leading_zeros() + 1) as u8;
        if r > self.registers[idx] {
            self.registers[idx] = r;
            true
        } else {
            false
        }
    }

    /// Estimate distinct count.
    pub fn estimate(&self) -> f64 {
        let m = self.m as f64;
        // Sum 2^-r_i, harmonic-style.
        let sum: f64 = self.registers.iter().map(|&r| 2f64.powi(-(r as i32))).sum();
        let raw = self.alpha * m * m / sum;

        // Linear counting at low cardinality. Threshold per Flajolet et al.
        let zeros = self.registers.iter().filter(|&&r| r == 0).count();
        if zeros > 0 && raw <= 2.5 * m {
            -m * (zeros as f64 / m).ln()
        } else {
            raw
        }
    }

    /// Merge another HLL of the same precision. Element-wise max over registers.
    pub fn merge(&mut self, other: &Self) -> Result<(), HllError> {
        if self.p != other.p {
            return Err(HllError::PrecisionMismatch {
                left: self.p,
                right: other.p,
            });
        }
        for (a, b) in self.registers.iter_mut().zip(other.registers.iter()) {
            if *b > *a {
                *a = *b;
            }
        }
        Ok(())
    }
}

impl HyperLogLog {
    /// Access the underlying register array. Used by the feature
    /// modules (sparse promotion, union/intersect) without making the
    /// field itself public.
    #[inline]
    #[allow(dead_code)] // used only by the feature modules (off under default features)
    pub(crate) fn registers(&self) -> &[u8] {
        &self.registers
    }
}

pub(crate) fn alpha_m(m: u32) -> f64 {
    match m {
        16 => 0.673,
        32 => 0.697,
        64 => 0.709,
        _ => 0.7213 / (1.0 + 1.079 / m as f64),
    }
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // FNV-1a's bit distribution is poor for short sequential keys; pipe
    // through a SplitMix64 finalizer so HLL's bucket index and leading-zero
    // extraction see a well-mixed value.
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    h
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Base HLL is zero-dep + std-only; each opt-in
// adds a focused capability under its own Cargo feature.
#[cfg(any(feature = "sparse", feature = "union-intersect"))]
pub mod features;

#[cfg(feature = "sparse")]
pub use features::sparse::SparseHyperLogLog;
#[cfg(feature = "union-intersect")]
pub use features::union_intersect::{estimate_intersect, estimate_union, intersect_error_bound};

#[cfg(test)]
#[path = "hll_tests.rs"]
mod hll_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
