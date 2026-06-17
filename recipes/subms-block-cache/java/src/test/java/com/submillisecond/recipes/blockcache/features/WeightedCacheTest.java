package com.submillisecond.recipes.blockcache.features;

import java.util.List;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class WeightedCacheTest {

    @Test
    void smallEntriesFitUnderCapacity() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(100, v -> v.length);
        List<WeightedCache.Evicted<Integer, byte[]>> ev1 = c.put(1, new byte[10]);
        List<WeightedCache.Evicted<Integer, byte[]>> ev2 = c.put(2, new byte[20]);
        assertTrue(ev1.isEmpty());
        assertTrue(ev2.isEmpty());
        assertEquals(30, c.usedBytes());
        assertEquals(2, c.size());
    }

    @Test
    void evictsToMakeRoom() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(50, v -> v.length);
        c.put(1, new byte[30]);
        c.put(2, new byte[10]);
        List<WeightedCache.Evicted<Integer, byte[]>> ev = c.put(3, new byte[30]);
        assertFalse(ev.isEmpty(), "should have evicted to fit 30+10+30 into 50");
        assertTrue(c.usedBytes() <= 50, "usedBytes=" + c.usedBytes());
    }

    @Test
    void entryLargerThanCapacityIsRejected() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(10, v -> v.length);
        List<WeightedCache.Evicted<Integer, byte[]>> ev = c.put(1, new byte[100]);
        assertEquals(1, ev.size());
        assertEquals(1, ev.get(0).key());
        assertEquals(0, c.usedBytes(), "rejected entry must not count");
        assertNull(c.get(1));
    }

    @Test
    void updateInPlaceAdjustsUsedBytes() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(100, v -> v.length);
        c.put(1, new byte[10]);
        c.put(1, new byte[40]);
        assertEquals(40, c.usedBytes());
        assertEquals(1, c.size());
        assertEquals(40, c.get(1).length);
    }

    @Test
    void updateBloatingEvictsOthers() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(60, v -> v.length);
        c.put(1, new byte[10]);
        c.put(2, new byte[10]);
        c.put(3, new byte[10]);
        List<WeightedCache.Evicted<Integer, byte[]>> ev = c.put(1, new byte[50]);
        assertFalse(ev.isEmpty());
        assertTrue(c.usedBytes() <= 60);
        assertNotNull(c.get(1));
    }

    @Test
    void touchedEntrySurvivesSweep() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(40, v -> v.length);
        c.put(1, new byte[10]);
        c.put(2, new byte[10]);
        c.put(3, new byte[10]);
        c.put(4, new byte[10]);
        c.put(5, new byte[10]); // first eviction; sweeps and clears refs.
        if (c.get(2) != null) {
            c.put(6, new byte[10]);
            assertNotNull(c.get(2), "touched key 2 should survive the next sweep");
        }
    }

    @Test
    void capacityBytesFloorIsOne() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(0, v -> v.length);
        assertEquals(1, c.capacityBytes());
    }

    @Test
    void isEmptyReflectsState() {
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(100, v -> v.length);
        assertTrue(c.isEmpty());
        c.put(1, new byte[10]);
        assertFalse(c.isEmpty());
    }

    @Test
    void allReferencedClearsBitsThenEvicts() {
        // Touch every key so every ref bit is set, then force eviction.
        // The sweep clears bits on first round, evicts on the second.
        WeightedCache<Integer, byte[]> c = new WeightedCache<>(40, v -> v.length);
        c.put(1, new byte[10]);
        c.put(2, new byte[10]);
        c.put(3, new byte[10]);
        c.put(4, new byte[10]);
        for (int k = 1; k <= 4; k++) c.get(k);
        var ev = c.put(5, new byte[10]);
        assertFalse(ev.isEmpty(), "should have evicted to make room");
        assertTrue(c.usedBytes() <= 40);
    }
}
