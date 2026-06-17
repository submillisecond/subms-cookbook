package com.submillisecond.recipes.cms.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class WindowedCountMinSketchTest {

    @Test
    void addVisibleInCurrentAndTotal() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(4, 5, 1024);
        for (int i = 0; i < 100; i++) w.add("k");
        assertTrue(w.estimateCurrent("k") >= 100);
        assertTrue(w.estimate("k") >= 100);
    }

    @Test
    void oldestSliceDropsOffAfterFullRotation() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(3, 5, 1024);
        for (int i = 0; i < 50; i++) w.add("k");
        assertTrue(w.estimate("k") >= 50);
        w.tick();
        w.tick();
        w.tick();
        assertEquals(0, w.estimate("k"));
    }

    @Test
    void addsAfterTickLandInNewSlice() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(4, 5, 1024);
        for (int i = 0; i < 20; i++) w.add("a");
        w.tick();
        for (int i = 0; i < 30; i++) w.add("b");
        assertTrue(w.estimateCurrent("a") <= 1);
        assertTrue(w.estimateCurrent("b") >= 30);
        assertTrue(w.estimate("a") >= 20);
        assertTrue(w.estimate("b") >= 30);
    }

    @Test
    void emptyWindowEstimatesZero() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(4, 5, 1024);
        assertEquals(0, w.estimate("nothing"));
        assertEquals(0, w.estimateCurrent("nothing"));
    }

    @Test
    void sliceCountFloorIsTwo() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(0, 5, 1024);
        assertTrue(w.slices() >= 2);
    }

    @Test
    void estimateSumsAcrossActiveSlices() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(3, 5, 4096);
        for (int i = 0; i < 10; i++) w.add("x");
        w.tick();
        for (int i = 0; i < 10; i++) w.add("x");
        w.tick();
        for (int i = 0; i < 10; i++) w.add("x");
        assertTrue(w.estimate("x") >= 30);
    }
}
