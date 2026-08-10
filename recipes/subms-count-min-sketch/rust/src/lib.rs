//! Count-Min Sketch with conservative update and Kirsch-Mitzenmacher hashing.
//!
//! `d` rows of `w` counters; each insert increments only the minimum cell(s)
//! across the d rows. Query returns the minimum cell across the d rows.
//! Width is rounded up to a power of two so indexing is a bitmask, not a `%`.
//!
//! Estimates are one-sided: `estimate(k) >= true_count(k)` always, with the
//! over-count bounded by `relative_error() * total()` at `confidence()`.
//!
//! ```
//! use subms_count_min_sketch::CountMinSketch;
//!
//! // Size from the error budget rather than guessing (d, w): 0.1% of the
//! // stream volume, 99.9% of the time.
//! let mut cms = CountMinSketch::with_error_bounds(0.001, 0.999);
//! for _ in 0..1000 { cms.add("ESZ5"); }
//! for i in 0..50_000 { cms.add_u64(i); }
//!
//! let est = cms.estimate("ESZ5");
//! assert!(est >= 1000);
//! assert!(cms.estimate_lower_bound("ESZ5") <= 1000);
//! assert_eq!(cms.total(), 51_000);
//!
//! // Checkpoint and restore without a serialization dependency.
//! let bytes = cms.to_bytes();
//! let restored = CountMinSketch::from_bytes(&bytes).unwrap();
//! assert_eq!(restored.estimate("ESZ5"), est);
//! ```
//!
//! Not thread-safe. Every mutator takes `&mut self`, so a shared sketch needs
//! external synchronisation; the intended concurrent shape is one sketch per
//! writer thread folded with the `merge` feature at the join.
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-count-min-sketch>

use core::f64::consts::E;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Row cap. The add path keeps its index set in a fixed stack array, and
/// `d = 16` already puts the failure probability at `e^-16`, so a deeper
/// sketch buys nothing a wider one would not buy more cheaply.
pub const MAX_DEPTH: usize = 16;

const SNAPSHOT_MAGIC: [u8; 8] = *b"SUBMSCMS";
const SNAPSHOT_VERSION: u16 = 1;
const SNAPSHOT_HEADER: usize = 32;

/// Why a byte slice could not be decoded into a sketch.
#[derive(Debug, PartialEq, Eq)]
pub enum SnapshotError {
    BadMagic,
    UnsupportedVersion(u16),
    BadShape { depth: usize, width: usize },
    Truncated { expected: usize, actual: usize },
}

impl core::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SnapshotError::BadMagic => write!(f, "not a count-min-sketch snapshot"),
            SnapshotError::UnsupportedVersion(v) => write!(f, "unsupported snapshot version {v}"),
            SnapshotError::BadShape { depth, width } => {
                write!(f, "invalid shape: depth={depth}, width={width}")
            }
            SnapshotError::Truncated { expected, actual } => {
                write!(
                    f,
                    "truncated snapshot: expected {expected} bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

pub struct CountMinSketch {
    d: usize,
    w: usize,
    mask: usize,
    seed: u64,
    total: u64,
    rows: Vec<Vec<u32>>,
}

// Hand-written so a debug print stays a line rather than a d*w counter dump.
impl core::fmt::Debug for CountMinSketch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CountMinSketch")
            .field("depth", &self.d)
            .field("width", &self.w)
            .field("seed", &self.seed)
            .field("total", &self.total)
            .finish()
    }
}

impl CountMinSketch {
    /// `d` hash functions (rows, clamped to `2..=MAX_DEPTH`); `w` is rounded up
    /// to a power of two. Standard sizing d=5, w=16384 gives an additive error
    /// of at most `e/w` of the stream volume with probability `1 - e^-d`.
    pub fn new(d: usize, w: usize) -> Self {
        Self::with_seed(d, w, 0)
    }

    /// Same shape as [`CountMinSketch::new`], with the hash family shifted by
    /// `seed`. Two sketches only merge or compare if their seeds match.
    pub fn with_seed(d: usize, w: usize, seed: u64) -> Self {
        let d = d.clamp(2, MAX_DEPTH);
        let w = w.max(2).next_power_of_two();
        let rows = (0..d).map(|_| vec![0u32; w]).collect();
        Self {
            d,
            w,
            mask: w - 1,
            seed,
            total: 0,
            rows,
        }
    }

    /// Size from the error budget instead of from `(d, w)`. `epsilon` is the
    /// tolerated over-count as a fraction of total stream volume; `confidence`
    /// is the probability the bound holds.
    pub fn with_error_bounds(epsilon: f64, confidence: f64) -> Self {
        Self::with_error_bounds_seeded(epsilon, confidence, 0)
    }

    pub fn with_error_bounds_seeded(epsilon: f64, confidence: f64, seed: u64) -> Self {
        Self::with_seed(
            Self::suggest_depth(confidence),
            Self::suggest_width(epsilon),
            seed,
        )
    }

    /// Width needed for an additive error of `epsilon * total`: `ceil(e/epsilon)`,
    /// rounded up to a power of two.
    pub fn suggest_width(epsilon: f64) -> usize {
        if epsilon.is_nan() || epsilon <= 0.0 {
            return 1 << 30;
        }
        let w = (E / epsilon).ceil();
        if w >= (1u64 << 30) as f64 {
            return 1 << 30;
        }
        (w as usize).max(2).next_power_of_two()
    }

    /// Depth needed for the error bound to hold with probability `confidence`:
    /// `ceil(ln(1/(1-confidence)))`, clamped to `2..=MAX_DEPTH`.
    pub fn suggest_depth(confidence: f64) -> usize {
        if confidence.is_nan() || confidence <= 0.0 {
            return 2;
        }
        if confidence >= 1.0 {
            return MAX_DEPTH;
        }
        let d = (1.0 / (1.0 - confidence)).ln().ceil();
        if d >= MAX_DEPTH as f64 {
            return MAX_DEPTH;
        }
        (d as usize).clamp(2, MAX_DEPTH)
    }

    pub fn depth(&self) -> usize {
        self.d
    }
    pub fn width(&self) -> usize {
        self.w
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Total weight ingested, exactly. Unlike the per-key estimates this is a
    /// running sum, not a sketch, so it carries no error.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Additive error as a fraction of total volume: `e / w`.
    pub fn relative_error(&self) -> f64 {
        E / self.w as f64
    }

    /// Probability the error bound holds: `1 - e^-d`.
    pub fn confidence(&self) -> f64 {
        1.0 - (-(self.d as f64)).exp()
    }

    /// Absolute over-count budget at the current volume: `ceil(e/w * total)`.
    pub fn error_margin(&self) -> u32 {
        let m = (self.relative_error() * self.total as f64).ceil();
        if m >= u32::MAX as f64 {
            u32::MAX
        } else {
            m as u32
        }
    }

    /// Fraction of cells that have ever been touched. Climbing past ~0.5 means
    /// the sketch is undersized for the key cardinality it is seeing.
    /// O(d*w) - a monitoring call, not a hot-path one.
    pub fn occupancy(&self) -> f64 {
        let used: usize = self
            .rows
            .iter()
            .map(|r| r.iter().filter(|&&c| c != 0).count())
            .sum();
        used as f64 / (self.d * self.w) as f64
    }

    /// Counter-matrix footprint in bytes. Fixed at construction: the sketch
    /// never grows with key cardinality, which is the whole reason to use one.
    pub fn heap_bytes(&self) -> usize {
        self.d * self.w * core::mem::size_of::<u32>()
    }

    /// Increment the count of `key` by 1.
    pub fn add(&mut self, key: &str) {
        self.add_bytes_n(key.as_bytes(), 1);
    }

    /// Increment the count of `key` by `n`. A weighted update - notional,
    /// message bytes, filled quantity - not just an occurrence count.
    pub fn add_n(&mut self, key: &str, n: u32) {
        self.add_bytes_n(key.as_bytes(), n);
    }

    pub fn add_bytes(&mut self, key: &[u8]) {
        self.add_bytes_n(key, 1);
    }

    /// Increment by `n`. Conservative update: raise each of the `d` cells to
    /// `min + n` and leave any cell already above that alone. The min-query
    /// never reads those higher cells for this key, so raising them would only
    /// add slop for whatever else collides there.
    pub fn add_bytes_n(&mut self, key: &[u8], n: u32) {
        if n == 0 {
            return;
        }
        let (h1, h2) = self.hashes(key);
        let mut idxs = [0usize; MAX_DEPTH];
        let mut min = u32::MAX;
        for (i, slot) in idxs.iter_mut().take(self.d).enumerate() {
            let idx = self.cell_index(h1, h2, i);
            *slot = idx;
            min = min.min(self.rows[i][idx]);
        }
        let floor = min.saturating_add(n);
        for (i, &idx) in idxs.iter().take(self.d).enumerate() {
            if self.rows[i][idx] < floor {
                self.rows[i][idx] = floor;
            }
        }
        self.total = self.total.saturating_add(n as u64);
    }

    /// Increment an integer key by 1. Identical to hashing the key's
    /// little-endian bytes, without materialising them.
    pub fn add_u64(&mut self, key: u64) {
        self.add_u64_n(key, 1);
    }

    pub fn add_u64_n(&mut self, key: u64, n: u32) {
        self.add_bytes_n(&key.to_le_bytes(), n);
    }

    /// Estimated count for `key`. Always `>=` the true count; the over-count is
    /// bounded by [`CountMinSketch::error_margin`].
    pub fn estimate(&self, key: &str) -> u32 {
        self.estimate_bytes(key.as_bytes())
    }

    pub fn estimate_bytes(&self, key: &[u8]) -> u32 {
        let (h1, h2) = self.hashes(key);
        let mut min = u32::MAX;
        for i in 0..self.d {
            let idx = self.cell_index(h1, h2, i);
            min = min.min(self.rows[i][idx]);
        }
        min
    }

    pub fn estimate_u64(&self, key: u64) -> u32 {
        self.estimate_bytes(&key.to_le_bytes())
    }

    /// The other end of the interval: `estimate - error_margin`, floored at
    /// zero. The true count lies in `[lower_bound, estimate]` at `confidence()`.
    pub fn estimate_lower_bound(&self, key: &str) -> u32 {
        self.estimate(key).saturating_sub(self.error_margin())
    }

    /// Zero every counter and reset the volume. Shape and seed are kept, so a
    /// long-lived sketch can be recycled without reallocating the matrix.
    pub fn clear(&mut self) {
        for row in self.rows.iter_mut() {
            row.fill(0);
        }
        self.total = 0;
    }

    /// Snapshot to a self-describing byte buffer: a 32-byte header
    /// (magic, version, depth, width, seed, total) then `d * w` little-endian
    /// `u32` counters, row-major. Byte-identical to the Java port's output.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SNAPSHOT_HEADER + self.heap_bytes());
        out.extend_from_slice(&SNAPSHOT_MAGIC);
        out.extend_from_slice(&SNAPSHOT_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.d as u16).to_le_bytes());
        out.extend_from_slice(&(self.w as u32).to_le_bytes());
        out.extend_from_slice(&self.seed.to_le_bytes());
        out.extend_from_slice(&self.total.to_le_bytes());
        for row in &self.rows {
            for &cell in row {
                out.extend_from_slice(&cell.to_le_bytes());
            }
        }
        out
    }

    /// Inverse of [`CountMinSketch::to_bytes`]. Rejects a foreign or truncated
    /// buffer rather than decoding a plausible-looking sketch out of it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() < SNAPSHOT_HEADER {
            return Err(SnapshotError::Truncated {
                expected: SNAPSHOT_HEADER,
                actual: bytes.len(),
            });
        }
        if bytes[..8] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != SNAPSHOT_VERSION {
            return Err(SnapshotError::UnsupportedVersion(version));
        }
        let d = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
        let w = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        if !(2..=MAX_DEPTH).contains(&d) || w < 2 || !w.is_power_of_two() {
            return Err(SnapshotError::BadShape { depth: d, width: w });
        }
        let expected = SNAPSHOT_HEADER + d * w * 4;
        if bytes.len() != expected {
            return Err(SnapshotError::Truncated {
                expected,
                actual: bytes.len(),
            });
        }
        let seed = u64::from_le_bytes(bytes[16..24].try_into().expect("8 bytes"));
        let total = u64::from_le_bytes(bytes[24..32].try_into().expect("8 bytes"));

        let mut sketch = Self::with_seed(d, w, seed);
        let mut at = SNAPSHOT_HEADER;
        for row in sketch.rows.iter_mut() {
            for cell in row.iter_mut() {
                *cell = u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
                at += 4;
            }
        }
        sketch.total = total;
        Ok(sketch)
    }

    fn hashes(&self, key: &[u8]) -> (u64, u64) {
        // Two base hashes from one FNV-1a pass plus a finalizer mix.
        let h = mix(fnv1a64(key) ^ self.seed);
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
    pub(crate) fn apply_paired(&mut self, other: &CountMinSketch, sum: bool) {
        for (i, row) in self.rows.iter_mut().enumerate() {
            let src = &other.rows[i];
            for (cell, &s) in row.iter_mut().zip(src.iter()) {
                *cell = if sum {
                    cell.saturating_add(s)
                } else if s > *cell {
                    s
                } else {
                    *cell
                };
            }
        }
        self.total = self.total.saturating_add(other.total);
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
pub use features::merge::{MergeError, merge_disjoint_into, merge_into};
#[cfg(feature = "windowed")]
pub use features::windowed::WindowedCountMinSketch;

#[cfg(test)]
#[path = "cms_tests.rs"]
mod cms_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
