package com.submillisecond.recipes.cms.features;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HeavyHittersTest {

    @Test
    void emptyTopIsEmpty() {
        HeavyHitters hh = new HeavyHitters(5, 5, 1024);
        assertTrue(hh.top().isEmpty());
    }

    @Test
    void fewerThanKDistinctKeysAllPresent() {
        HeavyHitters hh = new HeavyHitters(10, 5, 1024);
        for (int i = 0; i < 100; i++) hh.add("a");
        for (int i = 0; i < 50; i++)  hh.add("b");
        for (int i = 0; i < 25; i++)  hh.add("c");
        List<HeavyHitters.Entry> top = hh.top();
        assertEquals(3, top.size());
        assertEquals("a", top.get(0).key);
        assertEquals("b", top.get(1).key);
        assertEquals("c", top.get(2).key);
        assertTrue(top.get(0).estimate >= top.get(1).estimate);
        assertTrue(top.get(1).estimate >= top.get(2).estimate);
    }

    @Test
    void coldKeysEvictedWhenHotterArrive() {
        HeavyHitters hh = new HeavyHitters(2, 5, 1024);
        for (int i = 0; i < 10;  i++) hh.add("cold");
        for (int i = 0; i < 20;  i++) hh.add("warm");
        for (int i = 0; i < 100; i++) hh.add("hot");
        List<HeavyHitters.Entry> top = hh.top();
        assertEquals(2, top.size());
        assertEquals("hot", top.get(0).key);
        boolean hasHot = false, hasWarm = false, hasCold = false;
        for (HeavyHitters.Entry e : top) {
            if (e.key.equals("hot"))  hasHot = true;
            if (e.key.equals("warm")) hasWarm = true;
            if (e.key.equals("cold")) hasCold = true;
        }
        assertTrue(hasHot);
        assertTrue(hasWarm);
        assertFalse(hasCold);
    }

    @Test
    void existingTopKeyRefreshedInPlace() {
        HeavyHitters hh = new HeavyHitters(3, 5, 1024);
        hh.add("a");
        hh.add("b");
        hh.add("c");
        for (int i = 0; i < 50; i++) hh.add("c");
        List<HeavyHitters.Entry> top = hh.top();
        assertEquals(3, top.size());
        assertEquals("c", top.get(0).key);
        assertTrue(top.get(0).estimate >= 50);
    }

    @Test
    void kOneTracksOnlyHottest() {
        HeavyHitters hh = new HeavyHitters(1, 5, 1024);
        for (int i = 0; i < 3;  i++) hh.add("low");
        for (int i = 0; i < 10; i++) hh.add("high");
        List<HeavyHitters.Entry> top = hh.top();
        assertEquals(1, top.size());
        assertEquals("high", top.get(0).key);
    }

    @Test
    void tiesDoNotChurnExistingEntries() {
        HeavyHitters hh = new HeavyHitters(2, 5, 1024);
        for (int i = 0; i < 5; i++) hh.add("first");
        for (int i = 0; i < 5; i++) hh.add("second");
        for (int i = 0; i < 5; i++) hh.add("third");
        List<HeavyHitters.Entry> top = hh.top();
        boolean hasFirst = false, hasSecond = false, hasThird = false;
        for (HeavyHitters.Entry e : top) {
            if (e.key.equals("first"))  hasFirst = true;
            if (e.key.equals("second")) hasSecond = true;
            if (e.key.equals("third"))  hasThird = true;
        }
        assertTrue(hasFirst);
        assertTrue(hasSecond);
        assertFalse(hasThird);
    }

    @Test
    void kFloorEnforced() {
        HeavyHitters hh = new HeavyHitters(0, 5, 1024);
        assertEquals(1, hh.k());
    }

    @Test
    void weightedAddRanksByWeightNotOccurrence() {
        HeavyHitters hh = new HeavyHitters(2, 5, 4096);
        for (int i = 0; i < 100; i++) hh.add("chatty-small");
        hh.addN("rare-huge", 5000);
        assertEquals("rare-huge", hh.top().get(0).key);
        assertEquals(5100L, hh.total());
    }

    @Test
    void zeroWeightAddDoesNotEnterTheBoard() {
        HeavyHitters hh = new HeavyHitters(3, 5, 1024);
        hh.addN("ghost", 0);
        assertTrue(hh.top().isEmpty());
        assertEquals(0L, hh.total());
    }

    @Test
    void clearDropsSketchAndBoard() {
        HeavyHitters hh = new HeavyHitters(3, 5, 1024);
        for (int i = 0; i < 100; i++) hh.add("a");
        hh.clear();
        assertTrue(hh.top().isEmpty());
        assertEquals(0, hh.estimate("a"));
        assertEquals(0L, hh.total());
    }

    @Test
    void backingSketchExposesTheSizing() {
        HeavyHitters hh = new HeavyHitters(3, 5, 8192, 42L);
        hh.add("a");
        assertEquals(5, hh.sketch().depth());
        assertEquals(8192, hh.sketch().width());
        assertEquals(42L, hh.sketch().seed());
        assertTrue(hh.sketch().confidence() > 0.99);
    }
}
