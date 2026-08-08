package com.submillisecond.recipes.stats;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsSamplesTest {

    @Test
    void emptySamplesReturnsZero() {
        SubMsSamples s = SubMsSamples.of(new long[0]);
        assertEquals(0, s.count());
        assertTrue(s.isEmpty());
        assertEquals(0L, s.p99());
        assertEquals(0L, s.mean());
        assertEquals(0L, s.stddev());
        assertEquals(0L, s.max());
    }

    @Test
    void knownDistributionPercentiles() {
        long[] raw = new long[100];
        for (int i = 0; i < 100; i++) raw[i] = i;
        SubMsSamples s = SubMsSamples.of(raw);
        assertEquals(100, s.count());
        assertEquals(50L, s.p50());
        assertEquals(99L, s.p99());
        assertEquals(99L, s.max());
    }

    @Test
    void cdfBucketsCountMatchesTotal() {
        long[] raw = new long[1000];
        for (int i = 0; i < 1000; i++) raw[i] = i + 1;
        long[] buckets = SubMsSamples.of(raw).cdfBuckets();
        long total = 0;
        for (long c : buckets) total += c;
        assertEquals(raw.length, total);
    }

    @Test
    void nullArrayTreatedAsEmpty() {
        SubMsSamples s = SubMsSamples.of(null);
        assertTrue(s.isEmpty());
        assertEquals(0L, s.p99());
    }

    @Test
    void delegatesToTailModule() {
        long[] raw = new long[1000];
        for (int i = 0; i < 1000; i++) raw[i] = 100L;
        SubMsSamples s = SubMsSamples.of(raw);
        assertTrue(Math.abs(s.tailFatnessRatio() - 1.0) < 0.01);
    }

    @Test
    void bootstrapCiNotEmpty() {
        long[] raw = new long[200];
        for (int i = 0; i < 200; i++) raw[i] = i;
        SubMsSamples s = SubMsSamples.of(raw);
        Bootstrap.CI ci = s.bootstrapPercentileCi(0.99, 100, 0.95, 7L);
        assertTrue(ci.lo() <= ci.hi());
    }
}
