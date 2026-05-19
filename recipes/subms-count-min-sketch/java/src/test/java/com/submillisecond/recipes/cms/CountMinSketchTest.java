package com.submillisecond.recipes.cms;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CountMinSketchTest {

    @Test
    void widthRoundedUpToPowerOfTwo() {
        assertEquals(1024, new CountMinSketch(5, 1000).width());
    }

    @Test
    void estimateAtOrAboveTrueCount() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        for (int i = 0; i < 1000; i++) cms.add("hot");
        for (int i = 0; i < 10; i++) cms.add("warm");
        assertTrue(cms.estimate("hot") >= 1000);
        assertTrue(cms.estimate("warm") >= 10);
        assertEquals(0, cms.estimate("absent"));
    }

    @Test
    void overEstimationBounded() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        for (int i = 0; i < 1000; i++) cms.add("k" + i);
        cms.add("HOT");
        int est = cms.estimate("HOT");
        assertTrue(est >= 1);
        assertTrue(est < 10, "got " + est);
    }

    @Test
    void depthFloorIsTwo() {
        assertTrue(new CountMinSketch(0, 1024).depth() >= 2);
    }

    @Test
    void unseenKeyReturnsZero() {
        assertEquals(0, new CountMinSketch(5, 16384).estimate("never"));
    }

    @Test
    void singleIncrementAtLeastOne() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        cms.add("only");
        assertTrue(cms.estimate("only") >= 1);
    }

    @Test
    void estimatesGrowMonotonically() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        int prev = 0;
        for (int i = 0; i < 1000; i++) {
            cms.add("rising");
            int cur = cms.estimate("rising");
            assertTrue(cur >= prev);
            prev = cur;
        }
    }

    @Test
    void manyDistinctKeysDontInflateUnrelated() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        cms.add("focus");
        for (int i = 0; i < 1000; i++) cms.add("noise-" + i);
        int est = cms.estimate("focus");
        assertTrue(est >= 1);
        assertTrue(est < 100, "over-estimation too high: " + est);
    }

    @Test
    void dAndWAccessors() {
        CountMinSketch cms = new CountMinSketch(7, 8192);
        assertEquals(7, cms.depth());
        assertEquals(8192, cms.width());
    }
}
