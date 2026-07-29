//! Count-Min Sketch with conservative update and Kirsch-Mitzenmacher hashing.
//!
//! `d` rows of `w` counters; each insert increments only the minimum cell(s)
//! across the d rows. Query returns the minimum cell across the d rows.
//! Width is rounded up to a power of two so indexing is a bitmask, not a `%`.
//!
//! ```
//! use subms_count_min_sketch::CountMinSketch;
//! let mut cms = CountMinSketch::new(5, 16384);
//! for _ in 0..1000 { cms.add("hello"); }
//! assert!(cms.estimate("hello") >= 1000);
//! ```

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

pub struct CountMinSketch {
    d: usize,
    w: usize,
    mask: usize,
    rows: Vec<Vec<u32>>,
}

impl CountMinSketch {
    /// `d` hash functions (rows); `w` is rounded up to a power of two.
    /// Standard: d=5, w=16384 gives error <= e/w with probability >= 1 - e^-d.
    pub fn new(d: usize, w: usize) -> Self {
        let d = d.max(2);
        let w = w.max(2).next_power_of_two();
        let rows = (0..d).map(|_| vec![0u32; w]).collect();
        Self {
            d,
            w,
            mask: w - 1,
            rows,
        }
    }

    pub fn depth(&self) -> usize {
        self.d
    }
    pub fn width(&self) -> usize {
        self.w
    }

    /// Increment the count of `key` by 1. Conservative update: increment only
    /// the cells equal to the current minimum across the d rows. Cuts
    /// over-estimation substantially versus naive update-all.
    pub fn add(&mut self, key: &str) {
        let (h1, h2) = self.hashes(key);
        // First pass: find minimum across the d cells.
        let mut min = u32::MAX;
        let mut idxs = [0usize; 16]; // d capped at 16 for stack indices
        let d = self.d.min(16);
        for (i, slot) in idxs.iter_mut().take(d).enumerate() {
            let idx = self.cell_index(h1, h2, i);
            *slot = idx;
            min = min.min(self.rows[i][idx]);
        }
        // Second pass: increment only cells at the minimum.
        let new_val = min.saturating_add(1);
        for (i, &idx) in idxs.iter().take(d).enumerate() {
            if self.rows[i][idx] == min {
                self.rows[i][idx] = new_val;
            }
        }
    }

    /// Estimated count for `key`. Always >= true count; over-estimation
    /// bounded by the standard CMS analysis.
    pub fn estimate(&self, key: &str) -> u32 {
        let (h1, h2) = self.hashes(key);
        let mut min = u32::MAX;
        let d = self.d.min(16);
        for i in 0..d {
            let idx = self.cell_index(h1, h2, i);
            min = min.min(self.rows[i][idx]);
        }
        min
    }

    fn hashes(&self, key: &str) -> (u64, u64) {
        // Two base hashes from one FNV-1a pass plus a finalizer mix.
        let h = mix(fnv1a64(key.as_bytes()));
        let h1 = h as u32 as u64;
        // h2 must be odd to keep the affine combination injective mod 2^k.
        let h2 = ((h >> 32) as u32 as u64) | 1;
        (h1, h2)
    }

    fn cell_index(&self, h1: u64, h2: u64, i: usize) -> usize {
        // Kirsch-Mitzenmacher: h_i = h1 + i * h2.
        let idx = h1.wrapping_add((i as u64).wrapping_mul(h2));
        (idx as usize) & self.mask
    }

    // Crate-private accessors used by features. Kept off the public
    // surface to avoid committing the row layout to downstream code.
    #[cfg(feature = "merge")]
    pub(crate) fn apply_paired_max(&mut self, other: &CountMinSketch) {
        // Caller has already validated shape.
        for (i, row) in self.rows.iter_mut().enumerate() {
            let src = &other.rows[i];
            for (cell, &s) in row.iter_mut().zip(src.iter()) {
                if s > *cell {
                    *cell = s;
                }
            }
        }
    }

    #[cfg(feature = "windowed")]
    pub(crate) fn clear(&mut self) {
        for row in self.rows.iter_mut() {
            for cell in row.iter_mut() {
                *cell = 0;
            }
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// SplitMix64 finalizer.
fn mix(mut h: u64) -> u64 {
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    h
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is independent and gated by its own
// Cargo feature; `cargo add subms-count-min-sketch` alone keeps the
// base zero-dep + std-only shape.
#[cfg(any(feature = "heavy-hitters", feature = "windowed", feature = "merge"))]
pub mod features;

#[cfg(feature = "heavy-hitters")]
pub use features::heavy_hitters::HeavyHitters;
#[cfg(feature = "merge")]
pub use features::merge::{MergeError, merge_into};
#[cfg(feature = "windowed")]
pub use features::windowed::WindowedCountMinSketch;

#[cfg(test)]
#[path = "cms_tests.rs"]
mod cms_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
