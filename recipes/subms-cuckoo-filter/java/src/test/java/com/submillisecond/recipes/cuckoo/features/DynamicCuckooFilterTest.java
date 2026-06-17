package com.submillisecond.recipes.cuckoo.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class DynamicCuckooFilterTest {

    @Test
    void roundTripBelowThreshold() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(2000);
        for (int i = 0; i < 500; i++) assertTrue(d.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(d.contains("k" + i));
        assertEquals(1, d.layerCount(), "no grow expected below threshold");
    }

    @Test
    void growsWhenThresholdCrossed() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(256, 0.5);
        for (int i = 0; i < 1000; i++) assertTrue(d.insert("k" + i));
        assertTrue(d.layerCount() >= 2, "expected growth, got " + d.layerCount());
        for (int i = 0; i < 1000; i++) assertTrue(d.contains("k" + i), "lost k" + i);
    }

    @Test
    void deleteWalksLayersNewestFirst() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(64, 0.25);
        for (int i = 0; i < 200; i++) d.insert("k" + i);
        assertTrue(d.layerCount() >= 2);
        for (int i = 0; i < 200; i++) assertTrue(d.delete("k" + i), "could not delete k" + i);
        assertEquals(0, d.size());
    }

    @Test
    void deleteUnknownReturnsFalse() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(100);
        d.insert("known");
        assertFalse(d.delete("never-inserted"));
        assertTrue(d.contains("known"));
    }

    @Test
    void emptyFilterRejectsAnything() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(100);
        assertFalse(d.contains("any"));
        assertTrue(d.isEmpty());
    }

    @Test
    void loadFactorResetsAfterGrow() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(64, 0.4);
        for (int i = 0; i < 200; i++) d.insert("k" + i);
        assertTrue(d.loadFactor() < 1.0);
    }

    @Test
    void cumulativeSizeTracksAllLayers() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(64, 0.3);
        int n = 500;
        for (int i = 0; i < n; i++) d.insert("k" + i);
        assertEquals(n, d.size());
        assertTrue(d.layerCount() >= 2);
    }

    @Test
    void invalidThresholdFallsBackToDefault() {
        DynamicCuckooFilter d = new DynamicCuckooFilter(100, Double.NaN);
        assertEquals(0.95, d.growThresholdForTest(), 1e-12);
        DynamicCuckooFilter d2 = new DynamicCuckooFilter(100, 1.5);
        assertEquals(0.95, d2.growThresholdForTest(), 1e-12);
    }
}
