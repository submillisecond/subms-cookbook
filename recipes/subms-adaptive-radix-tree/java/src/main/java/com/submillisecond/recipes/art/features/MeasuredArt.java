package com.submillisecond.recipes.art.features;

import com.submillisecond.recipes.art.Art;
import com.submillisecond.recipes.art.ArtInternals;

/**
 * {@code MeasuredArt<V>} wraps an {@link Art} and bumps per-op counters
 * on insert / get / delete. {@link #metrics()} returns an
 * {@link ArtMetrics} snapshot with lookup / insert / delete totals, the
 * depth of the last operation, and a Small/Full node-shape distribution.
 *
 * <p>Counter overflow: each counter is a {@code long}. {@code Long.MAX_VALUE}
 * only saturates after ~5.8 centuries at a billion ops/sec; treated as
 * effectively unbounded, but the {@code saturatingAdd} helper makes the
 * worst case explicit (saturate, not wrap).
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::metrics}.
 */
public final class MeasuredArt<V> {

    private final Art<V> inner;
    private long lookups;
    private long insertions;
    private long deletions;
    private int lastDepth;

    public MeasuredArt() {
        this.inner = new Art<>();
    }

    public int size() { return inner.size(); }
    public boolean isEmpty() { return inner.isEmpty(); }

    public V insert(byte[] key, V value) {
        insertions = saturatingAdd(insertions, 1);
        lastDepth = key.length;
        return inner.insert(key, value);
    }

    public V get(byte[] key) {
        lookups = saturatingAdd(lookups, 1);
        lastDepth = key.length;
        return inner.get(key);
    }

    public V delete(byte[] key) {
        deletions = saturatingAdd(deletions, 1);
        lastDepth = key.length;
        return ArtInternals.delete(inner, key);
    }

    /** Borrow the wrapped tree. Composes with the {@code Serialize} and
     *  {@code RangeScan} features. */
    public Art<V> tree() { return inner; }

    public ArtMetrics metrics() {
        int[] nt = ArtInternals.nodeTypeCounts(inner);
        return new ArtMetrics(
                lookups, insertions, deletions, lastDepth, nt[0], nt[1], nt[2], nt[3], inner.size());
    }

    // Package-private for the overflow test.
    void setLookupsForTest(long value) { this.lookups = value; }

    static long saturatingAdd(long a, long delta) {
        long sum = a + delta;
        if (((a ^ sum) & (delta ^ sum)) < 0) {
            // Signed overflow: signs of a and delta agreed, sign of sum flipped.
            return Long.MAX_VALUE;
        }
        return sum;
    }
}
