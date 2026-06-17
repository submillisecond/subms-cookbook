//! Sparse HyperLogLog encoding for low-cardinality streams. Stores a
//! `Vec<(register_index, rho)>` until the entry count crosses a
//! configured threshold, then promotes itself into a dense
//! `HyperLogLog` register array. Hot paths beyond promotion are the
//! same as the base.
//!
//! Why this exists: a default p=14 HLL allocates 16 KB for the
//! register array even when it has seen zero items. For pipelines
//! that maintain millions of small sketches keyed by tenant /
//! customer / shard, 16 KB × N quickly dominates memory. Sparse mode
//! starts at ~96 B and grows linearly until the dense crossover
//! point, where the dense array stops being an over-allocation.
//!
//! Crossover threshold defaults to `m / 4` entries. Past that, the
//! sparse list is approaching dense's memory cost without dense's
//! O(1) lookup, so promotion is the right move. Promotion is one-way
//! - we never go back from dense to sparse.

use crate::{HyperLogLog, alpha_m, fnv1a64};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// HyperLogLog variant that holds a compact `(idx, rho)` pair list at
/// low cardinality and promotes to a dense register array once the
/// list grows past a threshold.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

impl SparseHyperLogLog {
    /// New empty sparse-mode HLL at precision `p` (clamped to [4, 18])
    /// and default threshold `m / 4`. The crossover is a heuristic -
    /// past `m/4` entries the sparse list is ~1.5x the byte size of
    /// the dense array yet still O(N) for lookup.
    pub fn new(precision: u32) -> Self {
        let p = precision.clamp(4, 18);
        let m = 1u32 << p;
        let threshold = (m / 4) as usize;
        Self::with_threshold(p, threshold)
    }

    /// Explicit promotion threshold (in distinct register entries).
    /// Use this when you know the workload's cardinality envelope and
    /// want to delay or hasten the dense crossover.
    pub fn with_threshold(precision: u32, threshold: usize) -> Self {
        let p = precision.clamp(4, 18);
        let m = 1u32 << p;
        let alpha = alpha_m(m);
        Self {
            p,
            m,
            alpha,
            sparse: Some(Vec::new()),
            dense: None,
            threshold,
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

    /// Record a key. If we're sparse and the new entry pushes us past
    /// the threshold, promote to dense before returning.
    pub fn add(&mut self, key: &str) {
        let h = fnv1a64(key.as_bytes());
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
                }
            } else {
                list.push((idx, r));
                if list.len() >= self.threshold {
                    self.promote();
                }
            }
        } else if let Some(d) = self.dense.as_mut() {
            // After promotion, behave exactly like the dense base.
            // We re-derive the register write because the base's
            // `add()` API takes a `&str`, not the already-computed h.
            d.add(key);
        }
    }

    /// Estimate distinct count. In sparse mode the registers we don't
    /// hold are zero, so linear counting is exact under the HLL
    /// assumption that absent registers contribute log term `-m * ln(1)
    /// = 0`. We use the base HLL formula uniformly for consistency.
    pub fn estimate(&self) -> f64 {
        if let Some(list) = &self.sparse {
            let m = self.m as f64;
            // Σ 2^-r_i = sum over held entries + (m - len) * 2^0 for zero registers
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_sparse_and_grows_linearly() {
        let mut h = SparseHyperLogLog::new(10);
        assert!(h.is_sparse());
        for i in 0..20 {
            h.add(&format!("k{i}"));
        }
        assert!(h.is_sparse(), "still sparse below threshold");
        assert!(h.entry_count() > 0);
    }

    #[test]
    fn promotes_to_dense_at_threshold() {
        // p=8 -> m=256, default threshold = 64.
        let mut h = SparseHyperLogLog::new(8);
        // Push well past the threshold so promotion definitely fires.
        for i in 0..500 {
            h.add(&format!("k{i}"));
        }
        assert!(!h.is_sparse(), "promoted past threshold");
        assert!(h.as_dense().is_some());
    }

    #[test]
    fn estimate_matches_dense_after_promotion() {
        let mut sparse = SparseHyperLogLog::new(10);
        let mut dense = HyperLogLog::new(10);
        for i in 0..2_000 {
            let k = format!("user-{i}");
            sparse.add(&k);
            dense.add(&k);
        }
        let s = sparse.estimate();
        let d = dense.estimate();
        let rel = ((s - d).abs() / d.max(1.0)).abs();
        assert!(
            rel < 0.05,
            "post-promotion match within 5%: sparse={s} dense={d}"
        );
    }

    #[test]
    fn low_cardinality_accurate_via_linear_counting() {
        let mut h = SparseHyperLogLog::new(10);
        for i in 0..50 {
            h.add(&format!("k{i}"));
        }
        let est = h.estimate();
        assert!(
            est > 40.0 && est < 60.0,
            "low-card linear counting: got {est}"
        );
    }

    #[test]
    fn force_promote_idempotent() {
        let mut h = SparseHyperLogLog::new(8);
        h.add("a");
        h.promote();
        assert!(!h.is_sparse());
        h.promote();
        assert!(!h.is_sparse());
    }

    #[test]
    fn duplicate_keys_dont_inflate_entry_count() {
        let mut h = SparseHyperLogLog::new(10);
        for _ in 0..1_000 {
            h.add("same-key");
        }
        assert!(h.is_sparse());
        assert_eq!(
            h.entry_count(),
            1,
            "one register touched, regardless of insert count"
        );
    }
}
