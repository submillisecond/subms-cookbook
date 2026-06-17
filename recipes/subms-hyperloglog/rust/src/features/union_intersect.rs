//! Set operations on HyperLogLog sketches.
//!
//! - `estimate_union(a, b)` is exact in the HLL sense: merge the two
//!   sketches register-wise and estimate. Same operation as the base
//!   `merge()` method, just non-destructive.
//! - `estimate_intersect(a, b)` uses inclusion-exclusion:
//!   `|A and B| ~= |A| + |B| - |A or B|`. This is the only practical HLL
//!   intersection. Be aware: when A and B mostly overlap, the variance
//!   of the subtraction is large relative to the result, so the
//!   estimator gets noisy. The error bound is `~1.04/sqrt(m) * (|A| +
//!   |B|)`, not `~1.04/sqrt(m) * |A and B|`. For nearly-disjoint or
//!   nearly-identical sets, prefer Apache DataSketches' Theta sketches.

use crate::HyperLogLog;

/// `|A ∪ B|`, exact in the HLL sense.
pub fn estimate_union(a: &HyperLogLog, b: &HyperLogLog) -> Result<f64, &'static str> {
    if a.precision() != b.precision() {
        return Err("precision mismatch");
    }
    let mut merged = HyperLogLog::new(a.precision());
    let ra = a.registers();
    let rb = b.registers();
    // Reach into the merged buffer; the base `merge()` would work
    // too, but doing one pass keeps the cost obvious.
    merged.apply_paired_max(ra, rb);
    Ok(merged.estimate())
}

/// `|A ∩ B|` via inclusion-exclusion. Clamps to >= 0 since negative
/// estimates are a hard signal of large relative error.
pub fn estimate_intersect(a: &HyperLogLog, b: &HyperLogLog) -> Result<f64, &'static str> {
    let ea = a.estimate();
    let eb = b.estimate();
    let union = estimate_union(a, b)?;
    let inter = ea + eb - union;
    Ok(inter.max(0.0))
}

// Tiny extension on the base so we don't expose register internals
// to every caller. Lives here next to the only consumer.
impl HyperLogLog {
    pub(crate) fn apply_paired_max(&mut self, a: &[u8], b: &[u8]) {
        debug_assert_eq!(a.len(), self.registers().len());
        debug_assert_eq!(b.len(), self.registers().len());
        // Read-back via registers_mut() would be cleaner but the
        // field is `pub(crate)` already through the crate-private
        // `apply_sparse()` access pattern.
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let m = (*x).max(*y);
            if m > self.registers[i] {
                self.registers[i] = m;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_sets_intersect_near_zero() {
        let mut a = HyperLogLog::new(12);
        let mut b = HyperLogLog::new(12);
        for i in 0..5_000 {
            a.add(&format!("a-{i}"));
            b.add(&format!("b-{i}"));
        }
        let inter = estimate_intersect(&a, &b).unwrap();
        // The HLL variance bound here is ~1.04/sqrt(4096) * 10_000 ~= 162.
        // Generous: under 5% of either source.
        assert!(inter < 500.0, "disjoint intersection ~0, got {inter}");
    }

    #[test]
    fn identical_sets_intersect_near_cardinality() {
        let mut a = HyperLogLog::new(12);
        let mut b = HyperLogLog::new(12);
        for i in 0..5_000 {
            let k = format!("k-{i}");
            a.add(&k);
            b.add(&k);
        }
        let inter = estimate_intersect(&a, &b).unwrap();
        let rel = (inter - 5_000.0).abs() / 5_000.0;
        assert!(
            rel < 0.10,
            "identical intersection ≈ |A|, got {inter} (rel {rel:.3})"
        );
    }

    #[test]
    fn union_matches_merge() {
        let mut a = HyperLogLog::new(12);
        let mut b = HyperLogLog::new(12);
        for i in 0..3_000 {
            a.add(&format!("a-{i}"));
        }
        for i in 0..3_000 {
            b.add(&format!("b-{i}"));
        }
        let union = estimate_union(&a, &b).unwrap();
        let mut merged = HyperLogLog::new(12);
        merged.merge(&a).unwrap();
        merged.merge(&b).unwrap();
        let est = merged.estimate();
        let rel = (union - est).abs() / est.max(1.0);
        assert!(
            rel < 0.01,
            "union should equal merge: union={union} merged={est}"
        );
    }

    #[test]
    fn partial_overlap_makes_sense() {
        let mut a = HyperLogLog::new(13);
        let mut b = HyperLogLog::new(13);
        // 10k items in A. 10k items in B. 3k items in both.
        for i in 0..10_000 {
            a.add(&format!("a-{i}"));
        }
        for i in 0..10_000 {
            b.add(&format!("b-{i}"));
        }
        for i in 0..3_000 {
            let k = format!("both-{i}");
            a.add(&k);
            b.add(&k);
        }
        let inter = estimate_intersect(&a, &b).unwrap();
        let rel = (inter - 3_000.0).abs() / 3_000.0;
        // Inclusion-exclusion noise: allow 50% relative error at
        // this scale. That's the well-known weakness; the test
        // documents it rather than ignoring it.
        assert!(rel < 0.5, "3k overlap, got {inter} (rel {rel:.3})");
    }

    #[test]
    fn precision_mismatch_errors() {
        let a = HyperLogLog::new(12);
        let b = HyperLogLog::new(13);
        assert!(estimate_union(&a, &b).is_err());
        assert!(estimate_intersect(&a, &b).is_err());
    }
}
