package com.submillisecond.recipes.blockcache;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.blockcache.features.ArcCache;
import com.submillisecond.recipes.blockcache.features.MetricsCache;
import com.submillisecond.recipes.blockcache.features.ShardedCache;
import com.submillisecond.recipes.blockcache.features.TinyLfuCache;
import com.submillisecond.recipes.blockcache.features.WeightedCache;
import org.junit.jupiter.api.Test;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    private static String readBlock(long id) {
        return "page:" + id;
    }

    @Test
    void quickstart() {
        // quickstart:begin
        BlockCache<Integer, String> cache = new BlockCache<>(4);
        cache.put(1, "one");
        cache.put(2, "two");
        assertEquals("one", cache.get(1));   // a resident key never misses
        assertNull(cache.get(9));            // an absent key returns null
        // quickstart:end
    }

    @Test
    void basePageCacheScenario() {
        BlockCache<Long, String> cache = new BlockCache<>(4);
        for (long id = 100; id < 104; id++) {
            assertNull(cache.get(id), "cold on first touch");
            cache.put(id, readBlock(id));
        }
        assertEquals(4, cache.size());
        for (long id = 100; id < 104; id++) {
            assertNotNull(cache.get(id), "a resident page must never miss");
        }
        BlockCache.Evicted<Long, String> ev = cache.put(200L, readBlock(200));
        assertNotNull(ev, "a full cache evicts to admit a new page");
        assertEquals(4, cache.size(), "capacity is a hard bound");
    }

    @Test
    void arcHoldsFrequentSetThroughScan() {
        ArcCache<Long, String> cache = new ArcCache<>(8);
        for (long id = 0; id < 4; id++) {
            cache.put(id, readBlock(id));
            cache.get(id);
        }
        assertEquals(4, cache.t2Len(), "second touch lifts pages into T2");
        for (long id = 1000; id < 1200; id++) cache.put(id, readBlock(id));
        int survivors = 0;
        for (long id = 0; id < 4; id++) if (cache.get(id) != null) survivors++;
        assertEquals(4, survivors, "the scan must not evict the frequent set");
    }

    @Test
    void tinylfuRejectsOneShotScanPages() {
        TinyLfuCache<Long, String> cache = new TinyLfuCache<>(64);
        for (int r = 0; r < 50; r++) for (long id = 0; id < 16; id++) cache.get(id);
        for (long id = 0; id < 16; id++) cache.put(id, readBlock(id));
        for (int r = 0; r < 50; r++) for (long id = 0; id < 16; id++) cache.get(id);
        long rejBefore = cache.rejections();
        for (long id = 1000; id < 3000; id++) cache.put(id, readBlock(id));
        assertTrue(cache.rejections() > rejBefore,
                "admission filter must reject some one-shot scan pages");
    }

    @Test
    void weightedBoundsBytesAndRejectsOversized() {
        WeightedCache<Long, byte[]> cache = new WeightedCache<>(4096, (byte[] page) -> page.length);
        for (long id = 0; id < 64; id++) {
            int size = 128 + (int) (id % 8) * 128;
            cache.put(id, new byte[size]);
        }
        assertTrue(cache.usedBytes() <= 4096, "byte budget is a hard bound");
        assertEquals(1, cache.put(999L, new byte[8192]).size(), "oversized page is rejected");
        assertNull(cache.get(999L));
    }

    @Test
    void shardedSurvivesConcurrentLoad() throws InterruptedException {
        ShardedCache<Long, Long> cache = new ShardedCache<>(1024, 8);
        assertEquals(8, cache.numShards());
        for (long id = 0; id < 256; id++) cache.put(id, id);

        java.util.List<Thread> threads = new java.util.ArrayList<>();
        for (int t = 0; t < 4; t++) {
            threads.add(new Thread(() -> {
                for (long id = 0; id < 4000; id++) cache.get(id % 256);
            }));
        }
        for (int w = 0; w < 2; w++) {
            final long base = (long) w * 10_000;
            threads.add(new Thread(() -> {
                for (long i = 0; i < 2000; i++) cache.put(base + i, i);
            }));
        }
        for (Thread th : threads) th.start();
        for (Thread th : threads) th.join();
        assertTrue(cache.size() <= 1024, "capacity holds across all shards");
    }

    @Test
    void metricsReportsHitRatio() {
        MetricsCache<Long, String> cache = new MetricsCache<>(4);
        for (long id = 0; id < 4; id++) cache.put(id, readBlock(id));
        cache.get(0L);
        cache.get(1L);
        cache.get(2L);
        cache.get(99L);
        assertEquals(3, cache.metrics().hits());
        assertEquals(1, cache.metrics().misses());
        assertTrue(Math.abs(cache.metrics().hitRatio() - 0.75) < 1e-9);
    }
}
