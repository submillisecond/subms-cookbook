//! Sparse HyperLogLog encoding for low-cardinality streams. Stores a
//! `Vec<(register_index, rho)>` until the entry count crosses a
//! configured threshold, then promotes itself into a dense
//! `HyperLogLog` register array. Hot paths beyond promotion are the
//! same as the base.
//!
//! Why this exists: a default p=14 HLL allocates 16 KB for the
//! register array even when it has seen zero items. For pipelines
//! that maintain millions of small sketches keyed by tenant /
//! customer / shard, 16 KB per sketch quickly dominates memory. Sparse
//! mode starts at zero payload and grows five bytes per distinct
//! register touched, until the dense array stops being an
//! over-allocation.
//!
//! Crossover threshold defaults to `m / 4` entries. Past that, the
//! sparse list is past dense's memory cost without dense's O(1)
//! lookup, so promotion is the right move. Promotion is one-way - we
//! never go back from dense to sparse.
//!
//! This is the plain pair-list encoding, not HLL++'s. Heule et al.
//! store the sparse pairs at a higher temporary precision and
//! difference-encode them as varints behind a small unsorted temp set;
//! that buys accuracy and bytes at low cardinality and costs a merge
//! step on every flush. Neither is implemented here.

use crate::{HllError, HyperLogLog, alpha_m, fnv1a64};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// HyperLogLog variant that holds a compact `(idx, rho)` pair list at
/// low cardinality and promotes to a dense register array once the
/// list grows past a threshold.
///
/// Single-writer, same as the base type: `add`, `merge`, `clear` and
/// `promote` take `&mut self`.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub struct SparseHyperLogLog {
    p: u32,
    m: u32,
    alpha: f64,
    /// `None` once we've promoted to dense.
    sparse: Option<Vec<(u32, u8)>>,
    /// Populated after promotion; `None` while sparse.
    dense: Option<HyperLogLog>,
    /// Promotion threshold: number of distinct register indices
    /// allowed in the sparse representation before we materialise the
    /// full register array.
    threshold: usize,
}

impl core::fmt::Debug for SparseHyperLogLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SparseHyperLogLog")
            .field("p", &self.p)
            .field("sparse", &self.is_sparse())
            .field("entries", &self.entry_count())
            .field("estimate", &self.estimate())
            .finish()
    }
}

impl SparseHyperLogLog {
    /// New empty sparse-mode HLL at precision `p` (clamped to [4, 18])
    /// and default threshold `m / 4`.
    ///
    /// `m/4` is a round heuristic and it overshoots: at five bytes an entry
    /// the pair list reaches the dense array's byte cost at `m/5`, so between
    /// `m/5` and `m/4` a sparse sketch is both bigger and slower to probe.
    /// Pass `with_threshold(p, m / 5)` when bytes are what you are buying.
    pub fn new(precision: u32) -> Self {
        let p = precision.clamp(crate::MIN_PRECISION, crate::MAX_PRECISION);
        let m = 1u32 << p;
        let threshold = (m / 4) as usize;
        Self::with_threshold(p, threshold)
    }

    /// Explicit promotion threshold (in distinct register entries).
    /// Use this when you know the workload's cardinality envelope and
    /// want to delay or hasten the dense crossover.
    pub fn with_threshold(precision: u32, threshold: usize) -> Self {
        let p = precision.clamp(crate::MIN_PRECISION, crate::MAX_PRECISION);
        let m = 1u32 << p;
        let alpha = alpha_m(m);
        Self {
            p,
            m,
            alpha,
            sparse: Some(Vec::new()),
            dense: None,
            threshold: threshold.max(1),
        }
    }

    /// Wraps an already-dense sketch. Used by the wire codec when a buffer
    /// was written after promotion.
    pub(crate) fn from_dense(dense: HyperLogLog) -> Self {
        let p = dense.precision();
        let m = dense.register_count();
        Self {
            p,
            m,
            alpha: alpha_m(m),
            sparse: None,
            dense: Some(dense),
            threshold: (m / 4) as usize,
        }
    }

    pub fn precision(&self) -> u32 {
        self.p
    }
    pub fn register_count(&self) -> u32 {
        self.m
    }
    pub fn is_sparse(&self) -> bool {
        self.sparse.is_some()
    }
    /// Entry count at which this sketch promotes to dense.
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Analytic relative standard error once dense, `1.04 / sqrt(m)`. Sparse
    /// mode is tighter than this because linear counting over a mostly-empty
    /// register space is the accurate estimator down there; the number is the
    /// envelope the sketch converges to, not a bound on its current state.
    pub fn standard_error(&self) -> f64 {
        crate::RSE_CONSTANT / (self.m as f64).sqrt()
    }

    /// Payload cost of the representation, register array or pair list. This
    /// is the number the feature exists to move: at p=14 an untouched sparse
    /// sketch is a fraction of the dense 16384 bytes.
    ///
    /// Five bytes per entry, matching the wire encoding rather than the
    /// allocator - a `Vec<(u32, u8)>` pads each pair to eight and Java's two
    /// parallel arrays do not, so a layout-exact number would disagree across
    /// the ports for no useful reason.
    pub fn state_bytes(&self) -> usize {
        match (&self.sparse, &self.dense) {
            (Some(list), _) => list.len() * 5,
            (None, Some(d)) => d.state_bytes(),
            _ => 0,
        }
    }

    /// True while nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        match (&self.sparse, &self.dense) {
            (Some(list), _) => list.is_empty(),
            (None, Some(d)) => d.is_empty(),
            _ => true,
        }
    }

    /// Reset to an empty sparse sketch, dropping the dense array if we had
    /// promoted. The threshold survives; a reused sketch keeps its sizing.
    pub fn clear(&mut self) {
        self.sparse = Some(Vec::new());
        self.dense = None;
    }

    /// Distinct register entries currently held. Once promoted to
    /// dense the answer is the count of non-zero registers.
    pub fn entry_count(&self) -> usize {
        if let Some(list) = &self.sparse {
            list.len()
        } else if let Some(d) = &self.dense {
            d.registers().iter().filter(|&&r| r != 0).count()
        } else {
            0
        }
    }

    /// Record a key. Returns true when the sketch changed. If we're sparse and
    /// the new entry pushes us past the threshold, promote to dense before
    /// returning.
    pub fn add(&mut self, key: &str) -> bool {
        self.add_bytes(key.as_bytes())
    }

    /// Record a 64-bit id without rendering it to a string.
    pub fn add_u64(&mut self, key: u64) -> bool {
        self.add_bytes(&key.to_be_bytes())
    }

    /// Record raw bytes.
    pub fn add_bytes(&mut self, key: &[u8]) -> bool {
        let h = fnv1a64(key);
        let idx = (h >> (64 - self.p)) as u32;
        let w = (h << self.p) | (1u64 << (self.p - 1));
        let r = (w.leading_zeros() + 1) as u8;
        if let Some(list) = self.sparse.as_mut() {
            // Linear-probe the sparse list. With list lengths bounded
            // by `threshold = m/4`, this is bounded work; at p=14
            // that's at most ~4k cells - well below the cost of the
            // dense array allocation we're trying to avoid.
            if let Some(pos) = list.iter().position(|(i, _)| *i == idx) {
                if r > list[pos].1 {
                    list[pos].1 = r;
                    return true;
                }
                false
            } else {
                list.push((idx, r));
                if list.len() >= self.threshold {
                    self.promote();
                }
                true
            }
        } else if let Some(d) = self.dense.as_mut() {
            d.add_bytes(key)
        } else {
            false
        }
    }

    /// Estimate distinct count. In sparse mode the registers we don't
    /// hold are zero, so linear counting is exact under the HLL
    /// assumption that absent registers contribute log term `-m * ln(1)
    /// = 0`. We use the base HLL formula uniformly for consistency.
    pub fn estimate(&self) -> f64 {
        if let Some(list) = &self.sparse {
            let m = self.m as f64;
            // sum(2^-r_i) = sum over held entries + (m - len) * 2^0 for zero registers
            let held_sum: f64 = list.iter().map(|(_, r)| 2f64.powi(-(*r as i32))).sum();
            let zero_count = self.m as usize - list.len();
            let sum = held_sum + zero_count as f64;
            let raw = self.alpha * m * m / sum;
            if zero_count > 0 && raw <= 2.5 * m {
                -m * (zero_count as f64 / m).ln()
            } else {
                raw
            }
        } else if let Some(d) = &self.dense {
            d.estimate()
        } else {
            0.0
        }
    }

    /// Merge another sparse sketch of the same precision. Two sparse lists
    /// combine entry-wise and may cross the threshold on the way, in which
    /// case the result promotes. Once either side is dense the merge runs on
    /// dense registers, which is where a fan-in of many shards ends up.
    pub fn merge(&mut self, other: &Self) -> Result<(), HllError> {
        if self.p != other.p {
            return Err(HllError::PrecisionMismatch {
                left: self.p,
                right: other.p,
            });
        }
        if other.dense.is_some() {
            self.promote();
        }
        if let Some(list) = self.sparse.as_mut() {
            let entries = other.sparse.as_ref().expect("other is sparse here");
            for &(idx, r) in entries {
                match list.iter().position(|(i, _)| *i == idx) {
                    Some(pos) => {
                        if r > list[pos].1 {
                            list[pos].1 = r;
                        }
                    }
                    None => list.push((idx, r)),
                }
            }
            if list.len() >= self.threshold {
                self.promote();
            }
            return Ok(());
        }
        let target = self.dense.as_mut().expect("promoted above");
        match &other.dense {
            Some(d) => target.merge(d),
            None => {
                let entries = other.sparse.as_ref().expect("sparse when not dense");
                target.apply_sparse(entries);
                Ok(())
            }
        }
    }

    /// Force promotion to dense even if below the threshold. Useful
    /// for benchmarking or for handing the inner dense HLL to a peer
    /// that does not understand sparse mode.
    pub fn promote(&mut self) {
        if self.dense.is_some() {
            return;
        }
        let list = self.sparse.take().unwrap_or_default();
        let mut dense = HyperLogLog::new(self.p);
        // Mutating registers needs a writable view; expose via a
        // dedicated promotion helper on the base.
        dense.apply_sparse(&list);
        self.dense = Some(dense);
    }

    /// View into the dense HLL after promotion. `None` while sparse.
    pub fn as_dense(&self) -> Option<&HyperLogLog> {
        self.dense.as_ref()
    }

    /// Materialise a dense copy without mutating this sketch. The bridge to
    /// `estimate_union` / `estimate_intersect`, which only take base sketches.
    pub fn to_dense(&self) -> HyperLogLog {
        match &self.dense {
            Some(d) => {
                let mut out = HyperLogLog::new(self.p);
                let _ = out.merge(d);
                out
            }
            None => {
                let mut out = HyperLogLog::new(self.p);
                if let Some(list) = &self.sparse {
                    out.apply_sparse(list);
                }
                out
            }
        }
    }

    pub(crate) fn entries(&self) -> Option<&[(u32, u8)]> {
        self.sparse.as_deref()
    }
}

// Tiny extension on the base type so the sparse promoter can seed
// register values without making the registers public globally.
impl HyperLogLog {
    /// Apply a sparse list of `(register_index, rho)` pairs to a
    /// fresh dense register array. Used by `SparseHyperLogLog::promote`.
    pub(crate) fn apply_sparse(&mut self, list: &[(u32, u8)]) {
        // Direct field access is fine inside the impl - the field is
        // private to the crate, this is the only writer outside the
        // base `add()` method.
        for &(idx, r) in list {
            let i = idx as usize;
            if i < self.registers.len() && r > self.registers[i] {
                self.registers[i] = r;
            }
        }
    }
}

mod wire {
    use super::SparseHyperLogLog;
    use crate::HllError;
    use crate::codec::{ENC_DENSE, ENC_SPARSE, HEADER_LEN, read_header, read_u32, write_header};

    impl SparseHyperLogLog {
        /// Serialise in whichever representation the sketch currently holds.
        /// A thin sketch stays thin on the wire; a promoted one writes the
        /// same dense buffer `HyperLogLog::to_bytes` would.
        pub fn to_bytes(&self) -> Vec<u8> {
            if let Some(d) = self.as_dense() {
                return d.to_bytes();
            }
            let entries = self.entries().unwrap_or(&[]);
            let mut out = Vec::with_capacity(HEADER_LEN + 8 + entries.len() * 5);
            write_header(&mut out, ENC_SPARSE, self.precision());
            out.extend_from_slice(&(self.threshold() as u32).to_be_bytes());
            out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
            for &(idx, r) in entries {
                out.extend_from_slice(&idx.to_be_bytes());
                out.push(r);
            }
            out
        }

        /// Parse either encoding. A dense buffer comes back as an
        /// already-promoted sketch, which is the honest reading: the writer
        /// had crossed the threshold and the reader inherits that.
        pub fn from_bytes(bytes: &[u8]) -> Result<Self, HllError> {
            let (encoding, p) = read_header(bytes)?;
            if encoding == ENC_DENSE {
                return Ok(Self::from_dense(crate::HyperLogLog::from_bytes(bytes)?));
            }
            if encoding != ENC_SPARSE {
                return Err(HllError::UnsupportedEncoding(encoding));
            }
            if bytes.len() < HEADER_LEN + 8 {
                return Err(HllError::Truncated {
                    expected: HEADER_LEN + 8,
                    actual: bytes.len(),
                });
            }
            let threshold = read_u32(bytes, HEADER_LEN) as usize;
            let count = read_u32(bytes, HEADER_LEN + 4) as usize;
            let expected = HEADER_LEN + 8 + count * 5;
            if bytes.len() < expected {
                return Err(HllError::Truncated {
                    expected,
                    actual: bytes.len(),
                });
            }
            let mut out = Self::with_threshold(p, threshold);
            let list = out.sparse.as_mut().expect("fresh sketch is sparse");
            for i in 0..count {
                let at = HEADER_LEN + 8 + i * 5;
                list.push((read_u32(bytes, at), bytes[at + 4]));
            }
            if list.len() >= out.threshold {
                out.promote();
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
#[path = "sparse_tests.rs"]
mod tests;
