package com.submillisecond.recipes.blockcache.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MetricsCacheTest {

    @Test
    void hitAndMissCounts() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(4);
        c.put(1, 10);
        c.get(1);
        c.get(1);
        c.get(999);
        c.get(42);
        assertEquals(2, c.metrics().hits());
        assertEquals(2, c.metrics().misses());
    }

    @Test
    void evictionsIncrementOnlyOnEviction() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(2);
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(0, c.metrics().evictions());
        c.put(3, 30);
        assertTrue(c.metrics().evictions() >= 1);
    }

    @Test
    void admissionsCountEachPut() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(4);
        for (int k = 0; k < 10; k++) c.put(k, k);
        assertEquals(10, c.metrics().admissions());
    }

    @Test
    void hitRatioHandlesZero() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(4);
        assertEquals(0.0, c.metrics().hitRatio());
    }

    @Test
    void hitRatioAfterMixedOps() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(4);
        c.put(1, 10);
        c.get(1);
        c.get(1);
        c.get(1);
        c.get(99);
        double r = c.metrics().hitRatio();
        assertTrue(Math.abs(r - 0.75) < 1e-9, "expected 0.75, got " + r);
    }

    @Test
    void contentionCounterIsAddressable() {
        MetricsCache.CacheMetrics m = new MetricsCache.CacheMetrics();
        m.recordContention();
        m.recordContention();
        assertEquals(2, m.contentionEvents());
    }

    @Test
    void metricsDefaultIsZero() {
        MetricsCache.CacheMetrics m = new MetricsCache.CacheMetrics();
        assertEquals(0, m.hits());
        assertEquals(0, m.misses());
        assertEquals(0, m.evictions());
        assertEquals(0, m.admissions());
        assertEquals(0, m.contentionEvents());
    }

    @Test
    void inspectorsReadable() {
        MetricsCache<Integer, Integer> c = new MetricsCache<>(4);
        assertEquals(4, c.capacity());
        assertEquals(0, c.size());
        assertTrue(c.isEmpty());
        c.put(1, 10);
        assertEquals(1, c.size());
        assertFalse(c.isEmpty());
    }
}
