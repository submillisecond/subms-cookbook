package com.submillisecond.recipes.lsm.features;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Optional;

/**
 * Bounded LRU block cache. Built on {@link LinkedHashMap} in access-order
 * mode so {@code get()} bumps recency for free and the eldest-entry hook
 * fires once capacity is reached.
 *
 * <p>Wraps the underlying map in a synchronized layer so the cache can be
 * shared across reader threads. Single-threaded callers can reach for the
 * map directly; the synchronization is cheap when uncontended.
 */
public final class LruBlockCache implements BlockCache {

    private final int capacity;
    private final Map<BlockKey, byte[]> map;
    private long hits;
    private long misses;

    public LruBlockCache(int capacity) {
        this.capacity = Math.max(1, capacity);
        this.map = new LinkedHashMap<>(this.capacity, 0.75f, /* accessOrder = */ true) {
            @Override
            protected boolean removeEldestEntry(Map.Entry<BlockKey, byte[]> eldest) {
                return size() > LruBlockCache.this.capacity;
            }
        };
    }

    public int capacity() {
        return capacity;
    }

    public synchronized long hits() {
        return hits;
    }

    public synchronized long misses() {
        return misses;
    }

    @Override
    public synchronized Optional<byte[]> get(BlockKey key) {
        byte[] v = map.get(key);
        if (v != null) {
            hits++;
            return Optional.of(v);
        }
        misses++;
        return Optional.empty();
    }

    @Override
    public synchronized void put(BlockKey key, byte[] block) {
        map.put(key, block);
    }

    @Override
    public synchronized int size() {
        return map.size();
    }

    @Override
    public synchronized void clear() {
        map.clear();
    }
}
