package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HyperLogLog;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class UnionIntersectTest {

    @Test
    void disjointSetsIntersectNearZero() {
        HyperLogLog a = new HyperLogLog(12);
        HyperLogLog b = new HyperLogLog(12);
        for (int i = 0; i < 5_000; i++) {
            a.add("a-" + i);
            b.add("b-" + i);
        }
        double inter = UnionIntersect.estimateIntersect(a, b);
        assertTrue(inter < 500.0, "disjoint intersection ~0, got " + inter);
    }

    @Test
    void identicalSetsIntersectNearCardinality() {
        HyperLogLog a = new HyperLogLog(12);
        HyperLogLog b = new HyperLogLog(12);
        for (int i = 0; i < 5_000; i++) {
            String k = "k-" + i;
            a.add(k);
            b.add(k);
        }
        double inter = UnionIntersect.estimateIntersect(a, b);
        double rel = Math.abs(inter - 5_000.0) / 5_000.0;
        assertTrue(rel < 0.10, "identical intersection ≈ |A|, got " + inter);
    }

    @Test
    void unionMatchesMerge() {
        HyperLogLog a = new HyperLogLog(12);
        HyperLogLog b = new HyperLogLog(12);
        for (int i = 0; i < 3_000; i++) a.add("a-" + i);
        for (int i = 0; i < 3_000; i++) b.add("b-" + i);
        double union = UnionIntersect.estimateUnion(a, b);
        HyperLogLog merged = new HyperLogLog(12);
        merged.merge(a);
        merged.merge(b);
        double est = merged.estimate();
        double rel = Math.abs(union - est) / Math.max(est, 1.0);
        assertTrue(rel < 0.01, "union should equal merge: union=" + union + " merged=" + est);
    }

    @Test
    void partialOverlapMakesSense() {
        HyperLogLog a = new HyperLogLog(13);
        HyperLogLog b = new HyperLogLog(13);
        for (int i = 0; i < 10_000; i++) a.add("a-" + i);
        for (int i = 0; i < 10_000; i++) b.add("b-" + i);
        for (int i = 0; i < 3_000; i++) {
            String k = "both-" + i;
            a.add(k);
            b.add(k);
        }
        double inter = UnionIntersect.estimateIntersect(a, b);
        double rel = Math.abs(inter - 3_000.0) / 3_000.0;
        // Inclusion-exclusion noise: 50% relative error allowed.
        assertTrue(rel < 0.5, "3k overlap, got " + inter + " (rel " + rel + ")");
    }

    @Test
    void precisionMismatchThrows() {
        HyperLogLog a = new HyperLogLog(12);
        HyperLogLog b = new HyperLogLog(13);
        assertThrows(IllegalArgumentException.class, () -> UnionIntersect.estimateUnion(a, b));
        assertThrows(IllegalArgumentException.class, () -> UnionIntersect.estimateIntersect(a, b));
    }
}
