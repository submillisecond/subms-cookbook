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
#[path = "union_intersect_tests.rs"]
mod tests;
