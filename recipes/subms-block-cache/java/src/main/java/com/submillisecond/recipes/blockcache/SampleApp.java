package com.submillisecond.recipes.blockcache;

import com.submillisecond.recipes.blockcache.features.ArcCache;
import com.submillisecond.recipes.blockcache.features.MetricsCache;
import com.submillisecond.recipes.blockcache.features.ShardedCache;
import com.submillisecond.recipes.blockcache.features.TinyLfuCache;
import com.submillisecond.recipes.blockcache.features.WeightedCache;
import java.util.ArrayList;
import java.util.List;

/**
 * Sample app: a tour of {@code subms-block-cache}, base API first, then each
 * optional variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.blockcache.SampleApp}
 *
 * <p>The scenario throughout is a hot block cache in front of a cold columnar
 * store - a market-data query engine reads fixed-size column blocks (pages) by
 * id, and the cache serves repeat reads without paying the cold fetch.
 *
 * <ul>
 *   <li>base              - bounded page cache, clock-sweep eviction
 *   <li>arc               - scan-resistant cache that holds the frequent set
 *   <li>tinylfu           - frequency-gated admission against a one-shot scan
 *   <li>weighted          - byte-budgeted eviction for variable-size pages
 *   <li>concurrent-shards - many query threads reading pages without contending
 *   <li>metrics           - hit-ratio observability on the cache
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        basePageCache();
        arcScanResistance();
        tinylfuAdmission();
        weightedByteBudget();
        shardedParallelReaders();
        metricsHitRatio();
    }

    /** Stand-in for a cold columnar-store fetch: decompress one page. */
    static String readBlock(long id) {
        return "page:" + id;
    }

    /** Base API: warm from the store on a miss, serve the hot set, evict when full. */
    static void basePageCache() {
        System.out.println("== base: block cache in front of a cold columnar store ==");
        final int cap = 4;
        BlockCache<Long, String> cache = new BlockCache<>(cap);

        long[] hot = {100, 101, 102, 103};
        int coldReads = 0;
        for (long id : hot) {
            if (cache.get(id) != null) throw new AssertionError("cold on first touch");
            cache.put(id, readBlock(id));
            coldReads++;
        }
        System.out.println("  warmed " + cache.size() + " pages, " + coldReads + " cold reads");
        if (cache.size() != cap) throw new AssertionError("cache should be full");

        int served = 0;
        for (long id : hot) {
            if (cache.get(id) == null) throw new AssertionError("resident page must never miss");
            served++;
        }
        System.out.println("  re-read the hot set: " + served + " served, 0 cold reads");

        BlockCache.Evicted<Long, String> ev = cache.put(200L, readBlock(200));
        if (ev == null) throw new AssertionError("full cache must evict to admit a new page");
        System.out.println("  admitted page 200, evicted page " + ev.key());
        if (cache.size() != cap) throw new AssertionError("capacity is a hard bound");

        // A compaction rewrote page 200, so the cached copy is stale. Invalidate
        // it rather than waiting for the hand to come round.
        if (cache.remove(200L) == null) throw new AssertionError("stale page must be dropped");
        System.out.println("  invalidated page 200, " + cache.size() + " pages resident");
        cache.clear();
        if (!cache.isEmpty()) throw new AssertionError("clear drops the whole segment");
        System.out.println("  segment dropped, cache empty, capacity still " + cap);
    }

    /** arc: a one-shot scan only touches T1, so the frequent set in T2 survives. */
    static void arcScanResistance() {
        System.out.println("\n== arc: scan-resistant page cache ==");
        ArcCache<Long, String> cache = new ArcCache<>(8);
        for (long id = 0; id < 4; id++) {
            cache.put(id, readBlock(id));
            cache.get(id); // second touch promotes into the frequent list
        }
        System.out.println("  frequent set in T2: " + cache.t2Len() + " pages");

        for (long id = 1000; id < 1200; id++) cache.put(id, readBlock(id));
        int survivors = 0;
        for (long id = 0; id < 4; id++) if (cache.get(id) != null) survivors++;
        System.out.println("  hot pages surviving a 200-page scan: " + survivors + "/4");
        if (survivors != 4) throw new AssertionError("ARC must hold the frequent set through a scan");
    }

    /** tinylfu: low-frequency scan pages lose the admission fight and never enter. */
    static void tinylfuAdmission() {
        System.out.println("\n== tinylfu: frequency-gated admission ==");
        TinyLfuCache<Long, String> cache = new TinyLfuCache<>(64);
        for (int r = 0; r < 50; r++) for (long id = 0; id < 16; id++) cache.get(id);
        for (long id = 0; id < 16; id++) cache.put(id, readBlock(id));
        for (int r = 0; r < 50; r++) for (long id = 0; id < 16; id++) cache.get(id);

        long rejBefore = cache.rejections();
        for (long id = 1000; id < 3000; id++) cache.put(id, readBlock(id));
        long rejected = cache.rejections() - rejBefore;
        System.out.println("  admissions " + cache.admissions() + ", scan pages rejected " + rejected);
        if (rejected <= 0) throw new AssertionError("admission filter must reject some one-shot scan pages");
    }

    /** weighted: budget is bytes, not slots; an oversized page is handed straight back. */
    static void weightedByteBudget() {
        System.out.println("\n== weighted: byte-budgeted page cache ==");
        WeightedCache<Long, byte[]> cache = new WeightedCache<>(4096, (byte[] page) -> page.length);

        int evictedTotal = 0;
        for (long id = 0; id < 64; id++) {
            int size = 128 + (int) (id % 8) * 128;
            evictedTotal += cache.put(id, new byte[size]).size();
        }
        System.out.println("  used " + cache.usedBytes() + " / 4096 bytes, "
                + cache.size() + " pages, " + evictedTotal + " evicted");
        if (cache.usedBytes() > 4096) throw new AssertionError("byte budget is a hard bound");

        List<WeightedCache.Evicted<Long, byte[]>> tooBig = cache.put(999L, new byte[8192]);
        if (tooBig.size() != 1) throw new AssertionError("oversized page is rejected, not admitted");
        if (cache.get(999L) != null) throw new AssertionError("rejected page must not be resident");
    }

    /** concurrent-shards: threads on different shards never block each other. */
    static void shardedParallelReaders() throws InterruptedException {
        System.out.println("\n== concurrent-shards: parallel query threads ==");
        ShardedCache<Long, Long> cache = new ShardedCache<>(1024, 8);
        System.out.println("  " + cache.numShards() + " shards");
        for (long id = 0; id < 256; id++) cache.put(id, id);

        List<Thread> threads = new ArrayList<>();
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
        System.out.println("  survived concurrent load, " + cache.size() + " pages resident");
        if (cache.size() > 1024) throw new AssertionError("capacity holds across all shards");
    }

    /** metrics: the hit ratio you watch to decide whether the cache earns its footprint. */
    static void metricsHitRatio() {
        System.out.println("\n== metrics: hit-ratio observability ==");
        MetricsCache<Long, String> cache = new MetricsCache<>(4);
        for (long id = 0; id < 4; id++) cache.put(id, readBlock(id));
        cache.get(0L);
        cache.get(1L);
        cache.get(2L);
        cache.get(99L);
        var m = cache.metrics();
        System.out.printf("  hits %d, misses %d, hit ratio %.2f%n", m.hits(), m.misses(), m.hitRatio());
        if (m.hits() != 3) throw new AssertionError("expected 3 hits");
        if (m.misses() != 1) throw new AssertionError("expected 1 miss");
        if (Math.abs(m.hitRatio() - 0.75) > 1e-9) throw new AssertionError("hit ratio should be 0.75");
    }
}
