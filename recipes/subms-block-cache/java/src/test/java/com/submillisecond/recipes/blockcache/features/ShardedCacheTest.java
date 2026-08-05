package com.submillisecond.recipes.blockcache.features;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ShardedCacheTest {

    @Test
    void shardCountRoundsToPowerOfTwo() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(64, 6);
        assertEquals(8, c.numShards());
    }

    @Test
    void putThenGetSameThread() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(64, 4);
        c.put(1, 10);
        c.put(2, 20);
        assertEquals(10, c.get(1));
        assertEquals(20, c.get(2));
        assertNull(c.get(999));
    }

    @Test
    void manyKeysDistributeAcrossShards() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(256, 8);
        for (int k = 0; k < 200; k++) c.put(k, k * 2);
        assertTrue(c.size() <= 256);
        int hits = 0;
        for (int k = 0; k < 200; k++) if (c.get(k) != null) hits++;
        assertTrue(hits > 0);
    }

    @Test
    void concurrentWritesDoNotCorrupt() throws InterruptedException {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(1024, 8);
        List<Thread> threads = new ArrayList<>();
        for (int t = 0; t < 4; t++) {
            final int tid = t;
            threads.add(new Thread(() -> {
                for (int i = 0; i < 5_000; i++) {
                    int k = tid * 100_000 + i;
                    c.put(k, k);
                }
            }));
        }
        for (Thread thr : threads) thr.start();
        for (Thread thr : threads) thr.join();
        assertTrue(c.size() <= 1024);
    }

    @Test
    void concurrentReadersAndWriters() throws InterruptedException {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(256, 8);
        for (int k = 0; k < 100; k++) c.put(k, k);
        List<Thread> threads = new ArrayList<>();
        for (int i = 0; i < 4; i++) {
            threads.add(new Thread(() -> {
                for (int k = 0; k < 2000; k++) c.get(k % 100);
            }));
        }
        for (int t = 0; t < 2; t++) {
            final int tid = t;
            threads.add(new Thread(() -> {
                for (int i = 0; i < 2000; i++) c.put(tid * 1000 + i, i);
            }));
        }
        for (Thread thr : threads) thr.start();
        for (Thread thr : threads) thr.join();
        assertTrue(c.size() <= 256);
    }

    @Test
    void singleShardStillCorrect() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(8, 1);
        for (int k = 0; k < 16; k++) c.put(k, k * 3);
        assertEquals(1, c.numShards());
        assertTrue(c.size() <= 8);
    }

    @Test
    void contentionCounterReadable() throws InterruptedException {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(64, 2);
        List<Thread> threads = new ArrayList<>();
        for (int t = 0; t < 4; t++) {
            final int tid = t;
            threads.add(new Thread(() -> {
                for (int i = 0; i < 500; i++) {
                    c.put(tid * 100_000 + i, i);
                    c.get(tid * 100_000 + i);
                }
            }));
        }
        for (Thread thr : threads) thr.start();
        for (Thread thr : threads) thr.join();
        long events = c.contentionEvents();
        assertTrue(events >= 0);
    }

    @Test
    void putReturnsEvictionWhenShardFull() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(4, 1);
        c.put(1, 1);
        c.put(2, 2);
        c.put(3, 3);
        c.put(4, 4);
        // Now full. Insert key 5; eviction should be reported.
        var ev = c.put(5, 5);
        assertNotNull(ev);
    }

    @Test
    void isEmptyReflectsState() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(8, 2);
        assertTrue(c.isEmpty());
        c.put(1, 10);
        assertEquals(false, c.isEmpty());
    }

    @Test
    void removeAndClearReachEveryShard() {
        ShardedCache<Integer, Integer> c = new ShardedCache<>(256, 4);
        for (int k = 0; k < 32; k++) c.put(k, k);
        assertEquals(7, c.remove(7));
        assertNull(c.get(7));
        assertNull(c.remove(7));
        c.clear();
        assertTrue(c.isEmpty());
        assertEquals(4, c.numShards(), "clear does not reshard");
    }
}
