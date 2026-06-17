package com.submillisecond.recipes.hdrhist.features;

import com.submillisecond.recipes.hdrhist.HdrHistogram;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class MergeTest {

    @Test
    void emptyIntoEmpty() {
        HdrHistogram a = new HdrHistogram(3);
        HdrHistogram b = new HdrHistogram(3);
        Merge.merge(a, b);
        assertEquals(0, a.count());
        assertEquals(0, a.max());
    }

    @Test
    void sumsCounts() {
        HdrHistogram a = new HdrHistogram(3);
        HdrHistogram b = new HdrHistogram(3);
        for (long v = 1; v <= 100; v++) {
            a.record(v);
            b.record(v);
        }
        Merge.merge(a, b);
        assertEquals(200, a.count());
    }

    @Test
    void equivalentToRecordingAllValues() {
        HdrHistogram a = new HdrHistogram(3);
        HdrHistogram b = new HdrHistogram(3);
        HdrHistogram single = new HdrHistogram(3);
        for (long v = 1; v <= 500; v++) {
            a.record(v);
            single.record(v);
        }
        for (long v = 501; v <= 1000; v++) {
            b.record(v);
            single.record(v);
        }
        Merge.merge(a, b);
        assertEquals(single.count(), a.count());
        assertEquals(single.valueAtPercentile(0.5), a.valueAtPercentile(0.5));
        assertEquals(single.valueAtPercentile(0.99), a.valueAtPercentile(0.99));
    }

    @Test
    void mismatchedPrecisionThrows() {
        HdrHistogram a = new HdrHistogram(2);
        HdrHistogram b = new HdrHistogram(4);
        assertThrows(IllegalArgumentException.class, () -> Merge.merge(a, b));
    }

    @Test
    void mergeGrowsDstToFitSrc() {
        HdrHistogram a = new HdrHistogram(3);
        a.record(1);
        HdrHistogram b = new HdrHistogram(3);
        long big = 100_000_000L;
        b.record(big);
        Merge.merge(a, b);
        assertEquals(2, a.count());
        assertTrue(a.max() >= big / 2, "merged max ~ big, got " + a.max());
    }

    @Test
    void mergePreservesDistributionShape() {
        HdrHistogram a = new HdrHistogram(3);
        HdrHistogram b = new HdrHistogram(3);
        for (int i = 0; i < 1000; i++) a.record(50);
        for (int i = 0; i < 1000; i++) b.record(500);
        Merge.merge(a, b);
        long p50 = a.valueAtPercentile(0.5);
        long p99 = a.valueAtPercentile(0.99);
        assertTrue(p50 < 100, "low half from a, got " + p50);
        assertTrue(p99 >= 400, "high tail from b, got " + p99);
    }
}
