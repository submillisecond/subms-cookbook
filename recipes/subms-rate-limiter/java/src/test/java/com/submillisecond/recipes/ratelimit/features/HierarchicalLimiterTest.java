package com.submillisecond.recipes.ratelimit.features;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

final class HierarchicalLimiterTest {

    private static HierarchicalLimiter build(
            TestClock clk,
            long parentCap,
            double parentRate,
            int children,
            long childCap,
            double childRate) {
        return new HierarchicalLimiter(
                parentCap, parentRate, children, childCap, childRate, () -> sharedClock(clk));
    }

    private static Clock sharedClock(TestClock clk) {
        return clk::nowNs;
    }

    @Test
    void parentCapsTotalAcrossChildren() {
        TestClock clk = new TestClock();
        HierarchicalLimiter h = build(clk, 5L, 0.0, 2, 10L, 0.0);
        int granted = 0;
        for (int i = 0; i < 10; i++) if (h.tryAcquire(0, 1L)) granted++;
        for (int i = 0; i < 10; i++) if (h.tryAcquire(1, 1L)) granted++;
        assertEquals(5, granted, "parent must cap total at 5");
    }

    @Test
    void childCapsIndependentWhenParentHasBudget() {
        TestClock clk = new TestClock();
        HierarchicalLimiter h = build(clk, 1000L, 0.0, 1, 3L, 0.0);
        for (int i = 0; i < 3; i++) assertTrue(h.tryAcquire(0, 1L));
        assertFalse(h.tryAcquire(0, 1L), "child capacity exhausted");
    }

    @Test
    void unknownChildIdRejects() {
        TestClock clk = new TestClock();
        HierarchicalLimiter h = build(clk, 10L, 0.0, 1, 5L, 0.0);
        assertFalse(h.tryAcquire(99, 1L), "out-of-range child id should reject");
        assertFalse(h.tryAcquire(-1, 1L), "negative child id should reject");
    }

    @Test
    void refillAfterParentExhaustionUnblocksChildren() {
        TestClock clk = new TestClock();
        // Parent 5 cap, 50/sec = 1 per 20 ms.
        HierarchicalLimiter h = build(clk, 5L, 50.0, 2, 10L, 0.0);
        for (int i = 0; i < 5; i++) assertTrue(h.tryAcquire(0, 1L));
        assertFalse(h.tryAcquire(1, 1L), "parent exhausted");
        clk.advanceMs(100L);
        int got = 0;
        for (int i = 0; i < 10; i++) if (h.tryAcquire(1, 1L)) got++;
        assertEquals(5, got, "exactly 5 parent tokens refilled");
    }

    @Test
    void batchAcquireAtomicAtBothLevels() {
        TestClock clk = new TestClock();
        HierarchicalLimiter h = build(clk, 10L, 0.0, 1, 10L, 0.0);
        assertTrue(h.tryAcquire(0, 7L));
        assertFalse(h.tryAcquire(0, 5L), "would exceed parent capacity");
        assertTrue(h.tryAcquire(0, 3L), "exactly 3 left should grant");
        assertFalse(h.tryAcquire(0, 1L));
    }

    @Test
    void parentAndChildAccessorsExposeUnderlyingBuckets() {
        TestClock clk = new TestClock();
        HierarchicalLimiter h = build(clk, 7L, 0.0, 2, 3L, 0.0);
        assertEquals(7L, h.parent().capacity());
        assertNotNull(h.child(0));
        assertEquals(3L, h.child(0).capacity());
        assertNull(h.child(99));
        assertEquals(2, h.numChildren());
    }
}
