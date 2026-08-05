package com.submillisecond.recipes.blockcache.features;

import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.recipes.blockcache.BlockCache;

/**
 * Per-instance counters: hits, misses, evictions, admissions, and
 * shard-contention events.
 *
 * <p>Wraps the base {@link BlockCache} and atomically increments the
 * appropriate counter on each operation. All counters are
 * {@code AtomicLong} so summary reads are safe under concurrent
 * single-writer access.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_block_cache::features::metrics::MetricsCache}.
 */
public final class MetricsCache<K, V> {

    public static final class CacheMetrics {
        private final AtomicLong hits = new AtomicLong();
        private final AtomicLong misses = new AtomicLong();
        private final AtomicLong evictions = new AtomicLong();
        private final AtomicLong admissions = new AtomicLong();
        private final AtomicLong contention = new AtomicLong();

        public long hits() { return hits.get(); }
        public long misses() { return misses.get(); }
        public long evictions() { return evictions.get(); }
        public long admissions() { return admissions.get(); }
        public long contentionEvents() { return contention.get(); }

        public double hitRatio() {
            long h = hits.get();
            long m = misses.get();
            long total = h + m;
            return total == 0 ? 0.0 : (double) h / (double) total;
        }

        public void recordHit() { hits.incrementAndGet(); }
        public void recordMiss() { misses.incrementAndGet(); }
        public void recordEviction() { evictions.incrementAndGet(); }
        public void recordAdmission() { admissions.incrementAndGet(); }
        public void recordContention() { contention.incrementAndGet(); }
    }

    private final BlockCache<K, V> inner;
    private final CacheMetrics metrics = new CacheMetrics();

    public MetricsCache(int capacity) {
        this.inner = new BlockCache<>(capacity);
    }

    public int capacity() { return inner.capacity(); }
    public int size() { return inner.size(); }
    public boolean isEmpty() { return inner.isEmpty(); }
    public CacheMetrics metrics() { return metrics; }

    public V get(K key) {
        V v = inner.get(key);
        if (v != null) metrics.recordHit(); else metrics.recordMiss();
        return v;
    }

    public BlockCache.Evicted<K, V> put(K key, V value) {
        BlockCache.Evicted<K, V> ev = inner.put(key, value);
        metrics.recordAdmission();
        if (ev != null) metrics.recordEviction();
        return ev;
    }

    /** Invalidation is not eviction, so it moves no counter. */
    public V remove(K key) { return inner.remove(key); }

    public void clear() { inner.clear(); }
}
