package com.submillisecond.recipes.hdrhist;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HdrHistogramTest {

    @Test
    void emptyReturnsZero() {
        HdrHistogram h = new HdrHistogram(3);
        assertEquals(0, h.count());
        assertEquals(0, h.max());
        assertEquals(0, h.valueAtPercentile(0.99));
    }

    @Test
    void recordsAndCounts() {
        HdrHistogram h = new HdrHistogram(3);
        for (long v : new long[]{10, 20, 30, 40, 50}) h.record(v);
        assertEquals(5, h.count());
        assertTrue(h.max() >= 50);
    }

    @Test
    void percentilesMatchDistribution() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i);
        long p50 = h.valueAtPercentile(0.50);
        long p99 = h.valueAtPercentile(0.99);
        assertTrue(p50 >= 450 && p50 <= 550, "p50=" + p50);
        assertTrue(p99 >= 950 && p99 <= 1050, "p99=" + p99);
    }

    @Test
    void handlesLargeValues() {
        HdrHistogram h = new HdrHistogram(3);
        long big = 1_000_000_000L;
        for (int i = 0; i < 99; i++) h.record(10);
        h.record(big);
        assertEquals(100, h.count());
        assertTrue(h.max() >= big * 0.99);
    }

    @Test
    void precisionIsClamped() {
        assertTrue(new HdrHistogram(0).subCount() >= 2);
        assertTrue(new HdrHistogram(99).subCount() > 0);
    }

    @Test
    void singleValueRecorded() {
        HdrHistogram h = new HdrHistogram(3);
        h.record(123);
        assertEquals(1, h.count());
        assertTrue(h.max() >= 123);
    }

    @Test
    void percentileZeroIsMinimum() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 100; i++) h.record(i);
        assertTrue(h.valueAtPercentile(0.0) <= 5);
    }

    @Test
    void percentileOneIsMaximum() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 100; i++) h.record(i);
        assertTrue(h.valueAtPercentile(1.0) >= 95);
    }

    @Test
    void countZeroOnCreate() {
        assertEquals(0, new HdrHistogram(3).count());
        assertEquals(0, new HdrHistogram(1).count());
        assertEquals(0, new HdrHistogram(5).count());
    }

    @Test
    void percentileClampedAboveOne() {
        HdrHistogram h = new HdrHistogram(3);
        h.record(42);
        assertEquals(h.valueAtPercentile(1.0), h.valueAtPercentile(1.5));
    }

    @Test
    void highVolumeRecord() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 0; i < 100_000; i++) h.record((i % 1000) + 1);
        assertEquals(100_000, h.count());
        long p50 = h.valueAtPercentile(0.5);
        assertTrue(p50 >= 400 && p50 <= 600, "p50=" + p50);
    }

    @Test
    void coNoCorrectionWhenValueAtOrBelowInterval() {
        HdrHistogram h = new HdrHistogram(3);
        h.recordWithExpectedInterval(5, 10);  // value < interval: no backfill
        h.recordWithExpectedInterval(10, 10); // value == interval: no backfill
        assertEquals(2, h.count(), "on-cadence samples add exactly one each");
    }

    @Test
    void coDisabledWhenIntervalZero() {
        HdrHistogram h = new HdrHistogram(3);
        h.recordWithExpectedInterval(1000, 0); // interval 0: plain record
        assertEquals(1, h.count());
    }

    @Test
    void coBackfillsTheStall() {
        HdrHistogram h = new HdrHistogram(3);
        // A 1000-unit op at a 10-unit expected cadence backfills 990, 980, ..., 10
        // (99) plus the 1000 itself.
        h.recordWithExpectedInterval(1000, 10);
        assertEquals(100, h.count(), "1 real + 99 synthetic samples");
    }

    @Test
    void coCorrectionLiftsTheTail() {
        HdrHistogram plain = new HdrHistogram(3);
        HdrHistogram corrected = new HdrHistogram(3);
        for (int i = 0; i < 1000; i++) {
            plain.record(10);
            corrected.recordWithExpectedInterval(10, 10);
        }
        plain.record(1000);
        corrected.recordWithExpectedInterval(1000, 10);

        long p99Plain = plain.valueAtPercentile(0.99);
        long p99Corrected = corrected.valueAtPercentile(0.99);
        assertTrue(p99Plain <= 20, "uncorrected p99 hides the stall: " + p99Plain);
        assertTrue(p99Corrected > 100,
                "corrected p99 reflects the requests the stall blocked: " + p99Corrected);
    }

    @Test
    void emptyStatsAreZero() {
        HdrHistogram h = new HdrHistogram(3);
        assertEquals(0, h.min());
        assertEquals(0.0, h.mean());
        assertEquals(0, h.countAtValue(42));
        assertEquals(0.0, h.percentileAtOrBelowValue(42));
    }

    @Test
    void minAndMeanTrackTheDistribution() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i);
        // Values below subCount are their own bucket, so 1..1000 is exact.
        assertEquals(1, h.min());
        double mean = h.mean();
        assertTrue(mean >= 500.0 && mean <= 501.0, "mean=" + mean);
    }

    @Test
    void countAtValueReadsOneBucket() {
        HdrHistogram h = new HdrHistogram(3);
        for (int i = 0; i < 7; i++) h.record(500);
        h.record(9_000_000L);
        assertEquals(7, h.countAtValue(500));
        assertEquals(0, h.countAtValue(501));
        assertEquals(0, h.countAtValue(Long.MAX_VALUE));
        assertEquals(1, h.countAtValue(9_000_000L));
    }

    @Test
    void percentileAtOrBelowValueInvertsThePercentileRead() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i);
        double q = h.percentileAtOrBelowValue(500);
        assertTrue(q >= 0.49 && q <= 0.51, "q=" + q);
        assertEquals(1.0, h.percentileAtOrBelowValue(1000));
        assertTrue(h.percentileAtOrBelowValue(Long.MAX_VALUE) >= 1.0);
    }

    @Test
    void footprintGrowsWithRangeNotVolume() {
        HdrHistogram small = new HdrHistogram(3);
        for (int i = 0; i < 100_000; i++) small.record(999);
        long base = small.footprintBytes();
        assertEquals(2048L * 8L, base, "array starts at subCount counters");

        HdrHistogram wide = new HdrHistogram(3);
        wide.record(1_000_000L);
        assertTrue(wide.footprintBytes() > base,
                "a wider range grows the array: " + wide.footprintBytes() + " vs " + base);
    }

    @Test
    void resetEmptiesWithoutShrinking() {
        HdrHistogram h = new HdrHistogram(3);
        for (long i = 1; i <= 1000; i++) h.record(i * 1000);
        long footprint = h.footprintBytes();
        h.reset();
        assertEquals(0, h.count());
        assertEquals(0, h.max());
        assertEquals(0, h.min());
        assertEquals(0, h.valueAtPercentile(0.99));
        assertEquals(footprint, h.footprintBytes(), "the array stays allocated");
        h.record(50);
        assertEquals(1, h.count());
        assertEquals(50, h.max());
    }
}
