package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HyperLogLog;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class SparseHyperLogLogTest {

    @Test
    void startsSparseAndGrowsLinearly() {
        SparseHyperLogLog h = new SparseHyperLogLog(10);
        assertTrue(h.isSparse());
        for (int i = 0; i < 20; i++) h.add("k" + i);
        assertTrue(h.isSparse(), "below threshold");
        assertTrue(h.entryCount() > 0);
    }

    @Test
    void promotesAtThreshold() {
        // p=8 -> m=256, default threshold=64.
        SparseHyperLogLog h = new SparseHyperLogLog(8);
        for (int i = 0; i < 500; i++) h.add("k" + i);
        assertFalse(h.isSparse(), "promoted past threshold");
        assertNotNull(h.asDense());
    }

    @Test
    void estimateMatchesDenseAfterPromotion() {
        SparseHyperLogLog sparse = new SparseHyperLogLog(10);
        HyperLogLog dense = new HyperLogLog(10);
        for (int i = 0; i < 2_000; i++) {
            String k = "user-" + i;
            sparse.add(k);
            dense.add(k);
        }
        double s = sparse.estimate();
        double d = dense.estimate();
        double rel = Math.abs(s - d) / Math.max(d, 1.0);
        assertTrue(rel < 0.05, "post-promotion match within 5%: sparse=" + s + " dense=" + d);
    }

    @Test
    void lowCardinalityAccurate() {
        SparseHyperLogLog h = new SparseHyperLogLog(10);
        for (int i = 0; i < 50; i++) h.add("k" + i);
        double est = h.estimate();
        assertTrue(est > 40.0 && est < 60.0, "low-card linear counting: got " + est);
    }

    @Test
    void forcePromoteIdempotent() {
        SparseHyperLogLog h = new SparseHyperLogLog(8);
        h.add("a");
        h.promote();
        assertFalse(h.isSparse());
        h.promote();
        assertFalse(h.isSparse());
    }

    @Test
    void duplicateKeysDontInflateEntryCount() {
        SparseHyperLogLog h = new SparseHyperLogLog(10);
        for (int i = 0; i < 1_000; i++) h.add("same-key");
        assertTrue(h.isSparse());
        assertEquals(1, h.entryCount(), "one register touched");
    }
}
