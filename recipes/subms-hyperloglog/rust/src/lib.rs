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

pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HyperLogLog {
    p: u32,
    m: u32,
    pub(crate) registers: Vec<u8>,
    alpha: f64,
}

impl HyperLogLog {
    /// New empty HLL at the given precision. `precision` is clamped to
    /// `[4, 18]`; 14 gives ~16k registers / ~16 KB / ~1% std error.
    pub fn new(precision: u32) -> Self {
        let p = precision.clamp(4, 18);
        let m = 1u32 << p;
        let alpha = alpha_m(m);
        Self {
            p,
            m,
            registers: vec![0u8; m as usize],
            alpha,
        }
    }

    pub fn precision(&self) -> u32 {
        self.p
    }
    pub fn register_count(&self) -> u32 {
        self.m
    }

    /// Record a key.
    pub fn add(&mut self, key: &str) {
        let h = fnv1a64(key.as_bytes());
        let idx = (h >> (64 - self.p)) as usize;
        // Use the remaining 64-p bits for the leading-zero count. Place a
        // sentinel 1 at the bottom so leading_zeros never exceeds (64-p).
        let w = (h << self.p) | (1u64 << (self.p - 1));
        let r = (w.leading_zeros() + 1) as u8;
        if r > self.registers[idx] {
            self.registers[idx] = r;
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
    pub fn merge(&mut self, other: &Self) -> Result<(), &'static str> {
        if self.p != other.p {
            return Err("precision mismatch");
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
pub use features::union_intersect::{estimate_intersect, estimate_union};
