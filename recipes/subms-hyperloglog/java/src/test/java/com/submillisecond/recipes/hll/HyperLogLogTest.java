package com.submillisecond.recipes.hll;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HyperLogLogTest {

    private static double rel(double actual, double expected) {
        return Math.abs(actual - expected) / expected;
    }

    @Test
    void precisionIsClamped() {
        assertEquals(4, new HyperLogLog(2).precision());
        assertEquals(18, new HyperLogLog(99).precision());
    }

    @Test
    void emptyEstimateIsZero() {
        assertTrue(new HyperLogLog(14).estimate() < 1.0);
    }

    @Test
    void smallCardinalityUsesLinearCounting() {
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < 100; i++) hll.add("k" + i);
        assertTrue(rel(hll.estimate(), 100.0) < 0.05);
    }

    @Test
    void mediumCardinalityWithinTwoPercent() {
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < 10_000; i++) hll.add("key" + i);
        assertTrue(rel(hll.estimate(), 10_000.0) < 0.02);
    }

    @Test
    void mergeEquivalentToCombined() {
        HyperLogLog a = new HyperLogLog(14);
        HyperLogLog b = new HyperLogLog(14);
        for (int i = 0; i < 5_000; i++) {
            a.add("A" + i);
            b.add("B" + i);
        }
        a.merge(b);
        assertTrue(rel(a.estimate(), 10_000.0) < 0.03);
    }

    @Test
    void mergeRejectsPrecisionMismatch() {
        HyperLogLog a = new HyperLogLog(14);
        HyperLogLog b = new HyperLogLog(12);
        assertThrows(IllegalArgumentException.class, () -> a.merge(b));
    }

    @Test
    void idempotentAddStaysFlat() {
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < 1000; i++) hll.add("same");
        assertTrue(hll.estimate() < 5.0);
    }

    @Test
    void registerCountMatchesPrecision() {
        assertEquals(16, new HyperLogLog(4).registerCount());
        assertEquals(1024, new HyperLogLog(10).registerCount());
        assertEquals(16384, new HyperLogLog(14).registerCount());
    }

    @Test
    void highCardinalityWithinThreePercent() {
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < 50_000; i++) hll.add("k-" + i);
        assertTrue(rel(hll.estimate(), 50_000.0) < 0.03);
    }

    @Test
    void mergeOverlappingDoesNotDoubleCount() {
        HyperLogLog a = new HyperLogLog(14);
        HyperLogLog b = new HyperLogLog(14);
        for (int i = 0; i < 5000; i++) {
            a.add("same-" + i);
            b.add("same-" + i);
        }
        a.merge(b);
        assertTrue(rel(a.estimate(), 5000.0) < 0.05);
    }
}
