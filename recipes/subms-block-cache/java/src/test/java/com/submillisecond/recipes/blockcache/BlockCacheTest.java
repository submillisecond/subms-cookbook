package com.submillisecond.recipes.blockcache;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class BlockCacheTest {

    @Test
    void getReturnsInsertedValue() {
        BlockCache<Integer, Integer> c = new BlockCache<>(4);
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(10, c.get(1));
        assertEquals(20, c.get(2));
        assertNull(c.get(999));
    }

    @Test
    void updateInPlace() {
        BlockCache<Integer, Integer> c = new BlockCache<>(4);
        c.put(1, 10);
        assertNull(c.put(1, 11));
        assertEquals(11, c.get(1));
        assertEquals(1, c.size());
    }

    @Test
    void evictsWhenFull() {
        BlockCache<Integer, Integer> c = new BlockCache<>(3);
        c.put(1, 10);
        c.put(2, 20);
        c.put(3, 30);
        BlockCache.Evicted<Integer, Integer> ev = c.put(4, 40);
        assertNotNull(ev);
        assertEquals(3, c.size());
        assertEquals(40, c.get(4));
    }

    @Test
    void touchedKeySurvivesNextEviction() {
        BlockCache<Integer, Integer> c = new BlockCache<>(3);
        c.put(1, 10);
        c.put(2, 20);
        c.put(3, 30);
        c.put(4, 40);
        c.get(4);
        c.put(5, 50);
        assertTrue(c.get(4) != null);
    }

    @Test
    void capacityFloorIsOne() {
        BlockCache<Integer, Integer> c = new BlockCache<>(0);
        assertEquals(1, c.capacity());
        c.put(1, 10);
        assertEquals(10, c.get(1));
    }

    @Test
    void sizeAndIsEmpty() {
        BlockCache<Integer, Integer> c = new BlockCache<>(4);
        assertTrue(c.isEmpty());
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(2, c.size());
        assertFalse(c.isEmpty());
    }

    @Test
    void getOnMissingReturnsNull() {
        BlockCache<Integer, Integer> c = new BlockCache<>(4);
        assertNull(c.get(42));
        c.put(1, 1);
        assertNull(c.get(42));
    }

    @Test
    void evictionsMatchInsertCountBeyondCapacity() {
        BlockCache<Integer, Integer> c = new BlockCache<>(3);
        int evictions = 0;
        for (int i = 0; i < 10; i++) {
            if (c.put(i, i) != null) evictions++;
        }
        assertEquals(7, evictions);
        assertEquals(3, c.size());
    }

    @Test
    void capacityOneEvictsOnEveryInsert() {
        BlockCache<Integer, Integer> c = new BlockCache<>(1);
        c.put(1, 10);
        BlockCache.Evicted<Integer, Integer> ev = c.put(2, 20);
        assertNotNull(ev);
        assertEquals(1, ev.key());
        assertEquals(10, ev.value());
        assertNull(c.get(1));
        assertEquals(20, c.get(2));
    }

    @Test
    void mixedWorkloadPreservesInvariants() {
        BlockCache<Integer, Integer> c = new BlockCache<>(5);
        for (int i = 0; i < 20; i++) {
            c.put(i, i * 10);
            if (i % 3 == 0) c.get(i / 2);
        }
        assertEquals(5, c.size());
    }

    @Test
    void removeReturnsValueAndFreesTheSlot() {
        BlockCache<Integer, Integer> c = new BlockCache<>(3);
        c.put(1, 10);
        c.put(2, 20);
        c.put(3, 30);
        assertEquals(20, c.remove(2));
        assertEquals(2, c.size());
        assertNull(c.get(2));
        // The vacated slot is reusable: the next insert takes the fill path
        // rather than sweeping, so nothing is evicted.
        assertNull(c.put(4, 40), "a hole absorbs the insert");
        assertEquals(3, c.size());
        assertEquals(10, c.get(1));
        assertEquals(40, c.get(4));
    }

    @Test
    void removeAbsentKeyIsANoOp() {
        BlockCache<Integer, Integer> c = new BlockCache<>(2);
        c.put(1, 10);
        assertNull(c.remove(9));
        assertEquals(1, c.size());
        assertEquals(10, c.remove(1));
        assertNull(c.remove(1), "a second remove finds nothing");
        assertTrue(c.isEmpty());
    }

    @Test
    void clearEmptiesTheCacheAndKeepsCapacity() {
        BlockCache<Integer, Integer> c = new BlockCache<>(3);
        for (int k = 1; k <= 5; k++) c.put(k, k * 10);
        assertEquals(3, c.size());
        c.clear();
        assertTrue(c.isEmpty());
        assertEquals(0, c.size());
        assertEquals(3, c.capacity(), "clear does not resize");
        assertNull(c.get(5));
        for (int k = 10; k <= 12; k++) assertNull(c.put(k, k));
        assertEquals(3, c.size());
    }
}
