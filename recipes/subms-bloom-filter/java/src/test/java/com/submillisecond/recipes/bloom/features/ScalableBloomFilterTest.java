package com.submillisecond.recipes.bloom.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ScalableBloomFilterTest {

    @Test
    void addsLayerWhenSaturated() {
        ScalableBloomFilter sb = new ScalableBloomFilter(10);
        for (int i = 0; i < 30; i++) sb.add("k" + i);
        assertTrue(sb.layerCount() >= 2, "layers=" + sb.layerCount());
    }

    @Test
    void earlierLayerKeysStillFindable() {
        ScalableBloomFilter sb = new ScalableBloomFilter(5);
        for (int i = 0; i < 50; i++) sb.add("k" + i);
        for (int i = 0; i < 5; i++) {
            assertTrue(sb.mightContain("k" + i), "lost k" + i);
        }
    }

    @Test
    void membershipInvariantNoFalseNegatives() {
        ScalableBloomFilter sb = new ScalableBloomFilter(100);
        for (int i = 0; i < 500; i++) sb.add("k" + i);
        for (int i = 0; i < 500; i++) {
            assertTrue(sb.mightContain("k" + i));
        }
    }

    @Test
    void totalCountTracksInserts() {
        ScalableBloomFilter sb = new ScalableBloomFilter(10);
        for (int i = 0; i < 25; i++) sb.add("k" + i);
        assertEquals(25, sb.totalCount());
    }

    @Test
    void emptyScalableRejectsAnything() {
        ScalableBloomFilter sb = new ScalableBloomFilter(100);
        assertFalse(sb.mightContain("never-added"));
    }

    @Test
    void growthFactorDefaultIsTwo() {
        ScalableBloomFilter sb = new ScalableBloomFilter(4);
        for (int i = 0; i < 5; i++) sb.add("k" + i);
        assertEquals(2, sb.layerCount());
    }

    @Test
    void clearCollapsesTheLayerTower() {
        ScalableBloomFilter sb = new ScalableBloomFilter(10);
        for (int i = 0; i < 100; i++) sb.add("k" + i);
        assertTrue(sb.layerCount() > 1, "precondition: tower grew");
        sb.clear();
        assertEquals(1, sb.layerCount(), "reset must collapse to one layer");
        assertEquals(0, sb.totalCount());
        assertFalse(sb.mightContain("k0"));
        sb.add("fresh");
        assertTrue(sb.mightContain("fresh"), "filter is usable after clear");
    }
}
