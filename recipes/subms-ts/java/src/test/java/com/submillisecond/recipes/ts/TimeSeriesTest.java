package com.submillisecond.recipes.ts;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

class TimeSeriesTest {

    private static final TsNumeric<Double> D = TsNumeric.DOUBLE;

    private static TsSeries<Double> seeded(long[] ts, double[] v) {
        TsSeries<Double> s = new TsSeries<>();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], v[i]);
        return s;
    }

    @Test
    void pushAndLen() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3}, new double[] {1.0, 2.0, 3.0});
        assertEquals(3, s.size());
        assertFalse(s.isEmpty());
        assertEquals(new TsPoint<>(1L, 1.0), s.first().orElseThrow());
        assertEquals(new TsPoint<>(3L, 3.0), s.last().orElseThrow());
    }

    @Test
    void emptySeriesQueriesAreEmpty() {
        TsSeries<Double> s = new TsSeries<>();
        assertTrue(s.isEmpty());
        assertTrue(s.first().isEmpty());
        assertTrue(s.last().isEmpty());
        assertTrue(s.nearest(0).isEmpty());
        assertTrue(s.min(D).isEmpty());
        assertTrue(s.mean(D).isEmpty());
    }

    @Test
    void pushRejectsOutOfOrder() {
        TsSeries<Double> s = new TsSeries<>();
        s.push(10, 1.0);
        TsException e = assertThrows(TsException.class, () -> s.push(5, 2.0));
        assertEquals(TsException.Kind.NOT_MONOTONIC, e.kind());
        assertEquals(10, e.last());
        assertEquals(5, e.got());
        // equal ts is allowed (non-decreasing)
        s.push(10, 3.0);
        assertEquals(2, s.size());
    }

    @Test
    void pushRejectsNanAndInf() {
        TsSeries<Double> s = new TsSeries<>();
        assertEquals(TsException.Kind.NULL_VALUE,
                assertThrows(TsException.class, () -> s.push(1, Double.NaN)).kind());
        assertEquals(TsException.Kind.NULL_VALUE,
                assertThrows(TsException.class, () -> s.push(1, Double.POSITIVE_INFINITY)).kind());
        assertEquals(0, s.size());
    }

    @Test
    void getAtExactAndMiss() {
        TsSeries<Double> s = seeded(new long[] {1, 3, 5}, new double[] {1.0, 3.0, 5.0});
        assertEquals(3.0, s.getAt(3).orElseThrow().value());
        assertTrue(s.getAt(4).isEmpty());
        assertTrue(s.getAt(0).isEmpty());
        assertTrue(s.getAt(6).isEmpty());
    }

    @Test
    void nearestBeforeAfterAndNearest() {
        TsSeries<Double> s = seeded(new long[] {10, 20, 30}, new double[] {1.0, 2.0, 3.0});
        assertEquals(20, s.nearestBefore(25).orElseThrow().ts());
        assertEquals(10, s.nearestBefore(10).orElseThrow().ts());
        assertTrue(s.nearestBefore(5).isEmpty());
        assertEquals(30, s.nearestAfter(25).orElseThrow().ts());
        assertEquals(30, s.nearestAfter(30).orElseThrow().ts());
        assertTrue(s.nearestAfter(31).isEmpty());
        assertEquals(20, s.nearest(24).orElseThrow().ts());
        assertEquals(30, s.nearest(26).orElseThrow().ts());
        // tie resolves to the earlier
        assertEquals(20, s.nearest(25).orElseThrow().ts());
    }

    @Test
    void rangeInclusiveBoundsAndEmpty() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3, 4}, new double[] {1.0, 2.0, 3.0, 4.0});
        List<Long> got = new ArrayList<>();
        for (TsPoint<Double> p : s.range(2, 3)) got.add(p.ts());
        assertEquals(List.of(2L, 3L), got);
        assertEquals(0, count(s.range(5, 9)));
        assertEquals(0, count(s.range(3, 1)));
        assertEquals(4, count(s.range(0, 100)));
    }

    @Test
    void aggregatesFullAndRanged() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3, 4}, new double[] {5.0, 1.0, 9.0, 3.0});
        assertEquals(1.0, s.min(D).orElseThrow());
        assertEquals(9.0, s.max(D).orElseThrow());
        assertEquals(18.0, s.sum(D));
        assertEquals(4.5, s.mean(D).orElseThrow());
        assertEquals(2, s.minPoint(D).orElseThrow().ts());
        assertEquals(3, s.maxPoint(D).orElseThrow().ts());
        assertEquals(1.0, s.rangeMin(2, 3, D).orElseThrow());
        assertEquals(9.0, s.rangeMax(2, 3, D).orElseThrow());
        assertEquals(10.0, s.rangeSum(2, 3, D));
        assertEquals(5.0, s.rangeMean(2, 3, D).orElseThrow());
    }

    @Test
    void deleteAtAndRange() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3, 4}, new double[] {1.0, 2.0, 3.0, 4.0});
        assertEquals(2.0, s.deleteAt(2).orElseThrow().value());
        assertEquals(3, s.size());
        assertTrue(s.getAt(2).isEmpty());
        assertEquals(2, s.deleteRange(3, 4));
        assertEquals(1, s.size());
        assertEquals(1, s.first().orElseThrow().ts());
        assertEquals(1, s.last().orElseThrow().ts());
    }

    @Test
    void deleteByValueAndValueRange() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3, 4}, new double[] {5.0, 1.0, 5.0, 9.0});
        assertEquals(2, s.deleteByValue(5.0));
        assertEquals(2, s.size());
        TsSeries<Double> s2 = seeded(new long[] {1, 2, 3, 4}, new double[] {1.0, 4.0, 7.0, 10.0});
        assertEquals(2, s2.deleteValueRange(4.0, 7.0, D));
        List<Double> vals = new ArrayList<>();
        for (TsPoint<Double> p : s2) vals.add(p.value());
        assertEquals(List.of(1.0, 10.0), vals);
    }

    @Test
    void truncateRetainPopClear() {
        TsSeries<Double> s = seeded(new long[] {1, 2, 3, 4, 5}, new double[] {1.0, 2.0, 3.0, 4.0, 5.0});
        assertEquals(2, s.truncateBefore(3));
        assertEquals(3, s.first().orElseThrow().ts());
        assertEquals(1, s.truncateAfter(4));
        assertEquals(4, s.last().orElseThrow().ts());
        assertEquals(1, s.retain(p -> p.value() != 3.0));
        assertEquals(1, s.size());
        assertEquals(4, s.popFirst().orElseThrow().ts());
        assertTrue(s.isEmpty());
        TsSeries<Double> s2 = seeded(new long[] {1, 2}, new double[] {1.0, 2.0});
        assertEquals(2, s2.popLast().orElseThrow().ts());
        s2.clear();
        assertTrue(s2.isEmpty());
    }

    @Test
    void fromPointsValidates() {
        TsSeries<Double> ok = TsSeries.fromPoints(List.of(new TsPoint<>(1L, 1.0), new TsPoint<>(2L, 2.0)));
        assertEquals(2, ok.size());
        TsException e = assertThrows(TsException.class,
                () -> TsSeries.fromPoints(List.of(new TsPoint<>(2L, 1.0), new TsPoint<>(1L, 2.0))));
        assertEquals(TsException.Kind.NOT_MONOTONIC, e.kind());
    }

    @Test
    void sealBoundaryIsTransparent() {
        // Cross the 64Ki seal threshold: queries + aggregates must stay correct
        // across the warm/head chunk boundary. TsSeriesL is the i64 fast path.
        long n = 70_000L;
        TsSeriesL s = new TsSeriesL();
        for (long i = 0; i < n; i++) s.push(i, i);
        assertEquals(n, s.size());
        assertEquals(0, s.first().orElseThrow().ts());
        assertEquals(n - 1, s.last().orElseThrow().ts());
        assertEquals(100L, s.getAt(100).orElseThrow().value());
        assertEquals(69_000L, s.getAt(69_000).orElseThrow().value());
        List<Long> span = s.rangeTimestamps(65_530, 65_540);
        List<Long> want = new ArrayList<>();
        for (long t = 65_530; t <= 65_540; t++) want.add(t);
        assertEquals(want, span);
        assertEquals(65_536, s.nearestBefore(65_536).orElseThrow().ts());
        assertEquals(n - 1, s.max().orElseThrow());
        assertEquals(45L, s.rangeSum(0, 9));
    }

    @Test
    void deleteAcrossSealBoundaryRechunks() {
        TsSeriesL s = new TsSeriesL();
        for (long i = 0; i < 70_000L; i++) s.push(i, i);
        int removed = s.deleteRange(0, 50_000);
        assertEquals(50_001, removed);
        assertEquals(70_000 - 50_001, s.size());
        assertEquals(50_001, s.first().orElseThrow().ts());
        assertEquals(69_999, s.last().orElseThrow().ts());
        assertEquals(60_000L, s.getAt(60_000).orElseThrow().value());
    }

    @Test
    void doubleSealBoundaryStaysCorrect() {
        // Exercise the TsSeriesD primitive fast path across the seal boundary.
        TsSeriesD s = TsSeriesD.withCapacity(70_000);
        for (long i = 0; i < 70_000L; i++) s.push(i, i * 0.5);
        assertEquals(70_000, s.size());
        assertEquals(65_536 * 0.5, s.getAt(65_536).orElseThrow().value());
        assertEquals(0.0, s.rangeMin(0, 10).orElseThrow());
        assertEquals(5.0, s.rangeMax(0, 10).orElseThrow());
        assertEquals((0 + 1 + 2 + 3) * 0.5, s.rangeSum(0, 3));
    }

    @Test
    void instantSugarRoundTrips() {
        java.time.Instant t = java.time.Instant.ofEpochSecond(1, 500);
        TsPoint<Double> p = TsPoint.atInstant(t, 1.5);
        assertEquals(1_000_000_500L, p.ts());
        assertEquals(t, p.instant());
        TsSeries<Double> s = seeded(new long[] {1_000_000_000L, 2_000_000_000L}, new double[] {1.0, 2.0});
        assertEquals(2, count(s.rangeInstant(java.time.Instant.ofEpochSecond(1), java.time.Instant.ofEpochSecond(2))));
    }

    @Test
    void aggregatesMatchReferenceAcrossChunks() {
        // > 2 * SEAL_CAP, not a multiple of 8, so the lane-summed kernel runs
        // across the warm + head boundary and through its scalar tail.
        final int n = 150_003;
        TsSeriesD s = TsSeriesD.withCapacity(n);
        long st = 0x1234_5678_9abc_def0L;
        double[] vals = new double[n];
        for (int i = 0; i < n; i++) {
            st = st * 6364136223846793005L + 1442695040888963407L;
            double v = ((st >>> 11) / (double) (1L << 53)) * 200.0 - 100.0;
            vals[i] = v;
            s.push(i, v);
        }
        double refSum = 0, refMin = Double.POSITIVE_INFINITY, refMax = Double.NEGATIVE_INFINITY;
        for (double v : vals) {
            refSum += v;
            if (v < refMin) refMin = v;
            if (v > refMax) refMax = v;
        }
        // min/max exact; lane-summed sum may reorder by an ULP.
        assertEquals(refMin, s.min().orElseThrow());
        assertEquals(refMax, s.max().orElseThrow());
        double tol = Math.abs(refSum) * 1e-12 + 1e-9;
        assertTrue(Math.abs(s.sum() - refSum) <= tol, "sum " + s.sum() + " vs " + refSum);
        assertTrue(Math.abs(s.mean().orElseThrow() - refSum / n) <= tol);

        int lo = 60_000, hi = 130_000;
        double rSum = 0, rMin = Double.POSITIVE_INFINITY, rMax = Double.NEGATIVE_INFINITY;
        for (int i = lo; i <= hi; i++) {
            rSum += vals[i];
            if (vals[i] < rMin) rMin = vals[i];
            if (vals[i] > rMax) rMax = vals[i];
        }
        assertEquals(rMin, s.rangeMin(lo, hi).orElseThrow());
        assertEquals(rMax, s.rangeMax(lo, hi).orElseThrow());
        assertTrue(Math.abs(s.rangeSum(lo, hi) - rSum) <= Math.abs(rSum) * 1e-12 + 1e-9);
    }

    private static int count(Iterable<?> it) {
        int n = 0;
        for (Object ignored : it) n++;
        return n;
    }
}
