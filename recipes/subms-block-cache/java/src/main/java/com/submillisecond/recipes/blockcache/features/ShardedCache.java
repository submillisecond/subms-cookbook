package com.submillisecond.recipes.blockcache.features;

import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;

import com.submillisecond.recipes.blockcache.BlockCache;

/**
 * Sharded block cache: splits the keyspace into N independent shards
 * by {@code hash(key) & (N - 1)}. Each shard wraps the base
 * {@link BlockCache} behind its own {@link ReentrantLock} so readers
 * and writers on different shards never contend.
 *
 * <p>Per-instance contention counter tracks {@code tryLock} failures;
 * exposed via {@link #contentionEvents()} for the metrics feature
 * integration. The lock-acquire path still completes (we fall through
 * to the blocking acquire) so correctness is unchanged - the counter
 * is observational.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_block_cache::features::concurrent_shards::ShardedCache}.
 */
public final class ShardedCache<K, V> {

    private final ReentrantLock[] locks;
    @SuppressWarnings({"rawtypes", "unchecked"})
    private final BlockCache[] shards;
    private final AtomicLong contention = new AtomicLong();
    private final int mask;

    public ShardedCache(int totalCapacity, int numShards) {
        int n = nextPowerOfTwo(Math.max(1, numShards));
        int perShard = Math.max(1, (totalCapacity + n - 1) / n);
        this.locks = new ReentrantLock[n];
        this.shards = new BlockCache[n];
        for (int i = 0; i < n; i++) {
            locks[i] = new ReentrantLock();
            shards[i] = new BlockCache<>(perShard);
        }
        this.mask = n - 1;
    }

    public int numShards() { return shards.length; }
    public long contentionEvents() { return contention.get(); }

    /** Snapshot aggregate length. Acquires every shard lock briefly. */
    public int size() {
        int total = 0;
        for (int i = 0; i < shards.length; i++) {
            locks[i].lock();
            try {
                total += shards[i].size();
            } finally {
                locks[i].unlock();
            }
        }
        return total;
    }
    public boolean isEmpty() { return size() == 0; }

    private int shardIndex(K key) {
        return key.hashCode() & mask;
    }

    @SuppressWarnings("unchecked")
    public V get(K key) {
        int idx = shardIndex(key);
        acquire(idx);
        try {
            return (V) shards[idx].get(key);
        } finally {
            locks[idx].unlock();
        }
    }

    @SuppressWarnings("unchecked")
    public BlockCache.Evicted<K, V> put(K key, V value) {
        int idx = shardIndex(key);
        acquire(idx);
        try {
            return shards[idx].put(key, value);
        } finally {
            locks[idx].unlock();
        }
    }

    private void acquire(int idx) {
        if (!locks[idx].tryLock()) {
            contention.incrementAndGet();
            locks[idx].lock();
        }
    }

    private static int nextPowerOfTwo(int n) {
        int r = 1;
        while (r < n) r <<= 1;
        return r;
    }
}
