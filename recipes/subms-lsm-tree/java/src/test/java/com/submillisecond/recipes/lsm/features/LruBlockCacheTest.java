package com.submillisecond.recipes.lsm.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class LruBlockCacheTest {

    @Test
    void missThenHit() {
        LruBlockCache c = new LruBlockCache(4);
        BlockKey k = new BlockKey(1, 0);
        assertTrue(c.get(k).isEmpty());
        c.put(k, "payload".getBytes());
        assertArrayEquals("payload".getBytes(), c.get(k).orElseThrow());
        assertEquals(1, c.hits());
        assertEquals(1, c.misses());
    }

    @Test
    void lruEvictsColdest() {
        LruBlockCache c = new LruBlockCache(2);
        BlockKey a = new BlockKey(1, 0);
        BlockKey b = new BlockKey(2, 0);
        BlockKey z = new BlockKey(3, 0);
        c.put(a, new byte[]{'A'});
        c.put(b, new byte[]{'B'});
        c.get(a); // 'b' is now LRU
        c.put(z, new byte[]{'Z'});
        assertTrue(c.get(a).isPresent(), "recently-used 'a' stays");
        assertFalse(c.get(b).isPresent(), "stale 'b' evicted");
        assertTrue(c.get(z).isPresent(), "newest 'z' kept");
    }

    @Test
    void putOfExistingKeyRefreshesValue() {
        LruBlockCache c = new LruBlockCache(2);
        BlockKey k = new BlockKey(1, 0);
        c.put(k, "old".getBytes());
        c.put(k, "new".getBytes());
        assertArrayEquals("new".getBytes(), c.get(k).orElseThrow());
        assertEquals(1, c.size());
    }

    @Test
    void clearDropsEverything() {
        LruBlockCache c = new LruBlockCache(4);
        c.put(new BlockKey(1, 0), new byte[]{'x'});
        c.put(new BlockKey(2, 0), new byte[]{'y'});
        assertEquals(2, c.size());
        c.clear();
        assertEquals(0, c.size());
        assertTrue(c.isEmpty());
    }

    @Test
    void capacityFloorIsOne() {
        LruBlockCache c = new LruBlockCache(0);
        assertEquals(1, c.capacity());
        c.put(new BlockKey(1, 0), new byte[]{'a'});
        c.put(new BlockKey(2, 0), new byte[]{'b'});
        assertEquals(1, c.size(), "cap-1 cache holds the latest only");
        assertTrue(c.get(new BlockKey(2, 0)).isPresent());
        assertFalse(c.get(new BlockKey(1, 0)).isPresent());
    }

    @Test
    void distinctKeysShareCacheWhenUnderCapacity() {
        LruBlockCache c = new LruBlockCache(4);
        for (int i = 0; i < 4; i++) c.put(new BlockKey(i, 0), new byte[]{(byte) i});
        for (int i = 0; i < 4; i++) assertTrue(c.get(new BlockKey(i, 0)).isPresent());
        assertEquals(4, c.size());
    }

    @Test
    void hitsAndMissesAreCounted() {
        LruBlockCache c = new LruBlockCache(4);
        BlockKey k = new BlockKey(1, 0);
        c.put(k, new byte[]{'v'});
        c.get(k);
        c.get(k);
        c.get(new BlockKey(99, 0));
        assertEquals(2, c.hits());
        assertEquals(1, c.misses());
    }
}
