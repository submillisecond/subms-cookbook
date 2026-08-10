package com.submillisecond.recipes.hll.features;

import com.submillisecond.recipes.hll.HllException;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

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
    @Test
    void clearReturnsAPromotedSketchToSparse() {
        SparseHyperLogLog h = new SparseHyperLogLog(10, 8);
        for (int i = 0; i < 50; i++) h.add("k" + i);
        assertFalse(h.isSparse());
        h.clear();
        assertTrue(h.isSparse());
        assertTrue(h.isEmpty());
        assertEquals(0, h.entryCount());
        assertEquals(8, h.threshold(), "clear keeps the sizing decision");
    }

    @Test
    void stateBytesStaysFarUnderDenseWhileThin() {
        SparseHyperLogLog h = new SparseHyperLogLog(14);
        for (int i = 0; i < 20; i++) h.add("cpty-" + i);
        assertTrue(h.isSparse());
        assertTrue(h.stateBytes() < 16_384 / 10,
            "a thin sketch must not approach the 16 KB dense cost, got " + h.stateBytes());
        h.promote();
        assertEquals(16_384, h.stateBytes());
    }

    @Test
    void mergeOfTwoSparseSketchesUnionsThem() {
        SparseHyperLogLog a = new SparseHyperLogLog(12, 4096);
        SparseHyperLogLog b = new SparseHyperLogLog(12, 4096);
        for (int i = 0; i < 300; i++) {
            a.add("k" + i);
            b.add("k" + (i + 200));
        }
        a.merge(b);
        assertTrue(a.isSparse(), "500 entries stays under a 4096 threshold");
        double est = a.estimate();
        assertTrue(est > 450.0 && est < 550.0, "500 distinct, got " + est);
    }

    @Test
    void mergeCanPushASparseSketchOverTheThreshold() {
        SparseHyperLogLog a = new SparseHyperLogLog(12, 64);
        SparseHyperLogLog b = new SparseHyperLogLog(12, 4096);
        for (int i = 0; i < 20; i++) a.add("a" + i);
        for (int i = 0; i < 100; i++) b.add("b" + i);
        assertTrue(a.isSparse());
        a.merge(b);
        assertFalse(a.isSparse(), "the combined list crosses 64 entries");
        double est = a.estimate();
        assertTrue(est > 100.0 && est < 140.0, "120 distinct, got " + est);
    }

    @Test
    void mergeWithADensePeerPromotesAndStillCounts() {
        SparseHyperLogLog a = new SparseHyperLogLog(12, 4096);
        SparseHyperLogLog b = new SparseHyperLogLog(12, 8);
        for (int i = 0; i < 50; i++) a.add("a" + i);
        for (int i = 0; i < 500; i++) b.add("b" + i);
        assertTrue(a.isSparse());
        assertFalse(b.isSparse());
        a.merge(b);
        assertFalse(a.isSparse());
        double est = a.estimate();
        assertTrue(est > 490.0 && est < 620.0, "550 distinct, got " + est);
    }

    @Test
    void mergeRejectsPrecisionMismatch() {
        SparseHyperLogLog a = new SparseHyperLogLog(12);
        SparseHyperLogLog b = new SparseHyperLogLog(10);
        HllException e = assertThrows(HllException.class, () -> a.merge(b));
        assertEquals(HllException.Kind.PRECISION_MISMATCH, e.kind());
    }

    @Test
    void addReportsWhetherTheSketchChanged() {
        SparseHyperLogLog h = new SparseHyperLogLog(12);
        assertTrue(h.add("cpty-1"));
        assertFalse(h.add("cpty-1"));
        h.promote();
        assertFalse(h.add("cpty-1"), "still idempotent after promotion");
        assertTrue(h.add("cpty-2"));
    }

    @Test
    void longAndBytesPathsLandOnTheSameRegisters() {
        SparseHyperLogLog a = new SparseHyperLogLog(12);
        SparseHyperLogLog b = new SparseHyperLogLog(12);
        a.addLong(42);
        b.addBytes(new byte[] {0, 0, 0, 0, 0, 0, 0, 42});
        assertArrayEquals(a.toDense().registers(), b.toDense().registers());
    }

    @Test
    void toDenseBridgesIntoTheSetOpsWithoutPromoting() {
        SparseHyperLogLog h = new SparseHyperLogLog(12);
        for (int i = 0; i < 40; i++) h.add("k" + i);
        HyperLogLog dense = h.toDense();
        assertTrue(h.isSparse(), "toDense is non-destructive");
        assertEquals(h.estimate(), dense.estimate(), h.estimate() * 1e-9);
    }

    @Test
    void standardErrorMatchesTheDenseEnvelope() {
        assertEquals(1.04 / Math.sqrt(16_384), new SparseHyperLogLog(14).standardError(), 1e-12);
    }

    @Test
    void toStringSummarisesTheShape() {
        String s = new SparseHyperLogLog(14).toString();
        assertTrue(s.startsWith("SparseHyperLogLog{p=14"), s);
    }
}
