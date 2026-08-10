package com.submillisecond.recipes.hll;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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
    @Test
    void tryNewRejectsOutOfRangePrecision() {
        HllException lo = assertThrows(HllException.class, () -> HyperLogLog.tryNew(3));
        assertEquals(HllException.Kind.INVALID_PRECISION, lo.kind());
        assertThrows(HllException.class, () -> HyperLogLog.tryNew(19));
        assertEquals(14, HyperLogLog.tryNew(14).precision());
    }

    @Test
    void mergeErrorNamesBothPrecisions() {
        HyperLogLog a = new HyperLogLog(14);
        HyperLogLog b = new HyperLogLog(12);
        HllException e = assertThrows(HllException.class, () -> a.merge(b));
        assertEquals(HllException.Kind.PRECISION_MISMATCH, e.kind());
        assertTrue(e.getMessage().contains("14"));
        assertTrue(e.getMessage().contains("12"));
    }

    @Test
    void addReportsWhetherTheSketchChanged() {
        HyperLogLog hll = new HyperLogLog(14);
        assertTrue(hll.add("first-sighting"), "a fresh key moves a register");
        assertFalse(hll.add("first-sighting"), "the same key cannot move it again");
    }

    @Test
    void emptyThenPopulatedThenCleared() {
        HyperLogLog hll = new HyperLogLog(12);
        assertTrue(hll.isEmpty());
        for (int i = 0; i < 1_000; i++) hll.add("k" + i);
        assertFalse(hll.isEmpty());
        assertTrue(hll.estimate() > 900.0);
        hll.clear();
        assertTrue(hll.isEmpty());
        assertTrue(hll.estimate() < 1.0, "cleared sketch estimates ~0");
        assertEquals(4096, hll.stateBytes(), "clear keeps the allocation");
    }

    @Test
    void stringBytesAndLongPathsAgree() {
        HyperLogLog a = new HyperLogLog(12);
        HyperLogLog b = new HyperLogLog(12);
        a.add("AAPL");
        b.addBytes("AAPL".getBytes(java.nio.charset.StandardCharsets.UTF_8));
        assertArrayEquals(a.registers(), b.registers());

        HyperLogLog c = new HyperLogLog(12);
        HyperLogLog d = new HyperLogLog(12);
        c.addLong(0x0123456789abcdefL);
        d.addBytes(new byte[] {0x01, 0x23, 0x45, 0x67, (byte) 0x89, (byte) 0xab, (byte) 0xcd, (byte) 0xef});
        assertArrayEquals(c.registers(), d.registers());
    }

    @Test
    void addLongCountsDistinctIdsWithoutFormatting() {
        HyperLogLog hll = new HyperLogLog(14);
        for (long id = 0; id < 50_000; id++) hll.addLong(id);
        assertTrue(rel(hll.estimate(), 50_000.0) < 0.03, "50k ids, got " + hll.estimate());
    }

    @Test
    void standardErrorTracksTheAnalyticEnvelope() {
        HyperLogLog hll = new HyperLogLog(14);
        assertEquals(0.008125, hll.standardError(), 1e-6);
        HyperLogLog coarse = new HyperLogLog(12);
        assertEquals(2.0, coarse.standardError() / hll.standardError(), 1e-9);
    }

    @Test
    void measuredErrorSitsInsideThreeStandardErrors() {
        HyperLogLog hll = new HyperLogLog(14);
        for (long i = 0; i < 200_000; i++) hll.addLong(i);
        double err = rel(hll.estimate(), 200_000.0);
        assertTrue(err < 3.0 * hll.standardError(),
            "measured " + err + " against 3 sigma " + (3.0 * hll.standardError()));
    }

    @Test
    void precisionForStandardErrorPicksTheCheapestThatFits() {
        assertEquals(14, HyperLogLog.precisionForStandardError(0.01));
        assertEquals(12, HyperLogLog.precisionForStandardError(0.02));
        assertEquals(18, HyperLogLog.precisionForStandardError(0.0001));
        int p = HyperLogLog.precisionForStandardError(0.01);
        assertTrue(new HyperLogLog(p).standardError() <= 0.01);
    }

    @Test
    void stateBytesIsFixedRegardlessOfStreamLength() {
        HyperLogLog hll = new HyperLogLog(14);
        int before = hll.stateBytes();
        for (long i = 0; i < 100_000; i++) hll.addLong(i);
        assertEquals(before, hll.stateBytes());
        assertEquals(16_384, before);
    }

    @Test
    void copyIsIndependentOfTheOriginal() {
        HyperLogLog a = new HyperLogLog(12);
        for (int i = 0; i < 500; i++) a.add("k" + i);
        HyperLogLog b = a.copy();
        assertEquals(a.estimate(), b.estimate());
        for (int i = 500; i < 2_000; i++) a.add("k" + i);
        assertTrue(a.estimate() > b.estimate() * 1.5, "the copy did not follow the original");
    }

    @Test
    void toStringSummarisesWithoutDumpingRegisters() {
        String s = new HyperLogLog(14).toString();
        assertTrue(s.startsWith("HyperLogLog{p=14"), s);
        assertTrue(s.length() < 80, "must not dump 16384 bytes: " + s.length());
    }
}
