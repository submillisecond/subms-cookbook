package com.submillisecond.stats;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HistogramTest {

    @Test
    void log2Placement() {
        long[] buckets = Histogram.cdfBuckets(new long[]{1, 2, 3, 4, 8, 100, 1_000_000});
        assertEquals(1L, buckets[0]);
        assertEquals(2L, buckets[1]);
        assertEquals(1L, buckets[2]);
        assertEquals(1L, buckets[3]);
        assertEquals(1L, buckets[6]);
        assertEquals(1L, buckets[19]);
    }

    @Test
    void emptyInputAllZero() {
        long[] buckets = Histogram.cdfBuckets(new long[0]);
        assertEquals(64, buckets.length);
        for (long c : buckets) assertEquals(0L, c);
    }

    @Test
    void totalMatchesSampleCount() {
        long[] raw = new long[1000];
        for (int i = 0; i < 1000; i++) raw[i] = i + 1;
        long[] buckets = Histogram.cdfBuckets(raw);
        long total = 0;
        for (long c : buckets) total += c;
        assertEquals(raw.length, total);
    }

    @Test
    void zeroValueLandsInBucketZero() {
        long[] buckets = Histogram.cdfBuckets(new long[]{0L, 0L, 0L});
        assertEquals(3L, buckets[0]);
    }

    @Test
    void hasExactly64Buckets() {
        assertEquals(64, Histogram.cdfBuckets(new long[]{42L}).length);
    }
}
