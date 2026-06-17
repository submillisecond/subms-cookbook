package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HyperLogLog;

/**
 * Set operations on HyperLogLog sketches.
 *
 * <ul>
 *   <li>{@link #estimateUnion} is exact in the HLL sense: a register-wise
 *       max merge before estimation. Same operation as
 *       {@link HyperLogLog#merge}, non-destructive.
 *   <li>{@link #estimateIntersect} uses inclusion-exclusion:
 *       {@code |A ∩ B| ≈ |A| + |B| - |A ∪ B|}. This is the only practical
 *       HLL intersection. When A and B mostly overlap, the variance of
 *       the subtraction is large relative to the result, so the estimator
 *       gets noisy. Error bound is
 *       {@code ~1.04/sqrt(m) * (|A| + |B|)}, not
 *       {@code ~1.04/sqrt(m) * |A ∩ B|}. For nearly-disjoint or nearly-
 *       identical sets, prefer Apache DataSketches' Theta sketches.
 * </ul>
 */
public final class UnionIntersect {
    private UnionIntersect() {}

    /** {@code |A ∪ B|}, exact in the HLL sense. */
    public static double estimateUnion(HyperLogLog a, HyperLogLog b) {
        if (a.precision() != b.precision()) {
            throw new IllegalArgumentException("precision mismatch");
        }
        HyperLogLog merged = new HyperLogLog(a.precision());
        merged.applyPairedMax(a.registers(), b.registers());
        return merged.estimate();
    }

    /**
     * {@code |A ∩ B|} via inclusion-exclusion. Clamps to {@code >= 0}
     * since a negative estimate is a hard signal of large relative
     * error - usually means A and B share too few items to recover
     * the intersection at this precision.
     */
    public static double estimateIntersect(HyperLogLog a, HyperLogLog b) {
        double ea = a.estimate();
        double eb = b.estimate();
        double union = estimateUnion(a, b);
        double inter = ea + eb - union;
        return inter > 0.0 ? inter : 0.0;
    }
}
