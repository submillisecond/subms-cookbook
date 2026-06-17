package com.submillisecond.recipes.tsdownsampler;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsPoint;

class TsDownsamplerTest {

    private static List<Long> tierTs(TsDownsampler d, int level) {
        List<Long> out = new ArrayList<>();
        for (TsPoint<Double> p : d.tier(level)) {
            out.add(p.ts());
        }
        return out;
    }

    private static List<Double> tierVals(TsDownsampler d, int level) {
        List<Double> out = new ArrayList<>();
        for (TsPoint<Double> p : d.tier(level)) {
            out.add(p.value());
        }
        return out;
    }

    @Test
    void singleTierBuckets() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        for (long ts = 0; ts < 25; ts++) {
            d.push(ts, (double) ts);
        }
        d.flush();
        assertEquals(3, d.tier(0).size());
        List<Long> ts = tierTs(d, 0);
        List<Double> vals = tierVals(d, 0);
        assertEquals(0L, ts.get(0));
        assertEquals(4.5, vals.get(0));
        assertEquals(10L, ts.get(1));
        assertEquals(14.5, vals.get(1));
        assertEquals(20L, ts.get(2));
        assertEquals(22.0, vals.get(2));
    }

    @Test
    void multiTierIndependentBuckets() {
        TsDownsampler d = new TsDownsampler(new long[] {10, 100});
        for (long ts = 0; ts < 250; ts++) {
            d.push(ts, 1.0);
        }
        d.flush();
        assertEquals(25, d.tier(0).size());
        assertEquals(3, d.tier(1).size());
        assertEquals(2, d.tierCount());
    }

    @Test
    void bucketStatsFull() {
        TsDownsampler d = new TsDownsampler(new long[] {100});
        d.push(0, 5.0);
        d.push(10, 1.0);
        d.push(20, 9.0);
        d.push(30, 3.0);
        TsBucketStats s = d.bucketStats(0, 50).orElseThrow();
        assertEquals(4, s.count());
        assertEquals(18.0, s.sum());
        assertEquals(1.0, s.min());
        assertEquals(9.0, s.max());
        assertEquals(3.0, s.last());
        assertEquals(4.5, s.mean());
    }

    @Test
    void bucketStatsClosedLookup() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        for (long ts = 0; ts < 25; ts++) {
            d.push(ts, (double) ts);
        }
        TsBucketStats s = d.bucketStats(0, 15).orElseThrow();
        assertEquals(10, s.count());
        assertEquals(10.0, s.min());
        assertEquals(19.0, s.max());
        assertEquals(19.0, s.last());
    }

    @Test
    void emptyBucketIsNone() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        d.push(0, 1.0);
        assertTrue(d.bucketStats(0, 1_000).isEmpty());
    }

    @Test
    void sparsePointsSkipEmptyBuckets() {
        TsDownsampler d = new TsDownsampler(new long[] {100});
        d.push(0, 1.0);
        d.push(500, 2.0);
        d.flush();
        assertEquals(2, d.tier(0).size());
        assertEquals(List.of(0L, 500L), tierTs(d, 0));
    }

    @Test
    void flushEmitsOpenBucket() {
        TsDownsampler d = new TsDownsampler(new long[] {100});
        d.push(0, 1.0);
        d.push(50, 3.0);
        assertEquals(0, d.tier(0).size());
        d.flush();
        assertEquals(1, d.tier(0).size());
        assertEquals(2.0, d.tier(0).first().orElseThrow().value());
    }

    @Test
    void tierDurationsReported() {
        TsDownsampler d = new TsDownsampler(new long[] {1_000_000_000L, 60_000_000_000L});
        assertEquals(1_000_000_000L, d.tierDuration(0));
        assertEquals(60_000_000_000L, d.tierDuration(1));
    }

    @Test
    void negativeTimestampsBucketCorrectly() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        d.push(-15, 1.0);
        d.push(-12, 2.0);
        d.push(-5, 3.0);
        d.flush();
        assertEquals(2, d.tier(0).size());
        assertEquals(List.of(-20L, -10L), tierTs(d, 0));
    }

    @Test
    void realisticTiers1s1m() {
        final long s = 1_000_000_000L;
        final long m = 60 * s;
        TsDownsampler d = new TsDownsampler(new long[] {s, m});
        long ts = 0L;
        while (ts < 3 * m) {
            d.push(ts, (double) (ts / s));
            ts += 100_000_000L;
        }
        d.flush();
        assertEquals(180, d.tier(0).size());
        assertEquals(3, d.tier(1).size());
    }

    @Test
    void bucketStatsOutOfRangeLevelIsEmpty() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        d.push(0, 1.0);
        assertTrue(d.bucketStats(5, 0).isEmpty());
        assertTrue(d.bucketStats(-1, 0).isEmpty());
    }

    @Test
    void emptyMeanIsZeroAndDurationFloored() {
        TsDownsampler d = new TsDownsampler(new long[] {0});
        assertEquals(1L, d.tierDuration(0));
        d.push(0, 4.0);
        TsBucketStats s = d.bucketStats(0, 0).orElseThrow();
        assertEquals(4.0, s.mean());
        assertEquals(1, s.count());
    }

    @Test
    void negativeBucketStatsClosedAndOpen() {
        TsDownsampler d = new TsDownsampler(new long[] {10});
        d.push(-15, 1.0);
        d.push(-12, 3.0);
        d.push(-5, 9.0);
        Optional<TsBucketStats> closed = d.bucketStats(0, -18);
        assertTrue(closed.isPresent());
        assertEquals(2, closed.get().count());
        assertEquals(2.0, closed.get().mean());
        Optional<TsBucketStats> open = d.bucketStats(0, -5);
        assertTrue(open.isPresent());
        assertEquals(9.0, open.get().last());
    }

    @Test
    void statsEqualityAndToString() {
        TsDownsampler d = new TsDownsampler(new long[] {100});
        d.push(0, 5.0);
        TsBucketStats a = d.bucketStats(0, 0).orElseThrow();
        TsBucketStats b = d.bucketStats(0, 50).orElseThrow();
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        assertTrue(a.toString().contains("count=1"));
        assertFalse(a.equals("not stats"));
        assertTrue(a.equals(a));
    }
}
