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
}
