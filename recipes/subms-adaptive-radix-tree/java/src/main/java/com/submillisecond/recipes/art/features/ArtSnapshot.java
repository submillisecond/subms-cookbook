package com.submillisecond.recipes.art.features;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import com.submillisecond.recipes.art.Art;
import com.submillisecond.recipes.art.ArtInternals;

/**
 * Read-only snapshot view of an ART. Readers can hold it across thread
 * boundaries and across mutations of the original tree. The snapshot is
 * cheap to query ({@code O(log n)} binary search on a sorted array) and
 * bounded in size: one entry per keyed value, not one per tree node.
 *
 * <p>Trade-off vs a pointer-versioned snapshot: this copies values
 * shallowly into an unmodifiable list. For tiny values the copy is the
 * same size as the pointer anyway; for large mutable values, wrap them
 * in an immutable holder before insert.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::concurrent_reads}.
 */
public final class ArtSnapshot<V> {

    private final byte[][] keys;
    private final Object[] values;

    private ArtSnapshot(byte[][] keys, Object[] values) {
        this.keys = keys;
        this.values = values;
    }

    public static <V> ArtSnapshot<V> fromTree(Art<V> tree) {
        List<byte[]> ks = new ArrayList<>();
        List<V> vs = new ArrayList<>();
        ArtInternals.collect(tree, ks, vs);
        // collect() is already in byte-lex key order.
        byte[][] kArr = ks.toArray(new byte[0][]);
        Object[] vArr = vs.toArray();
        return new ArtSnapshot<>(kArr, vArr);
    }

    public int size() { return keys.length; }
    public boolean isEmpty() { return keys.length == 0; }

    @SuppressWarnings("unchecked")
    public V get(byte[] key) {
        int idx = binarySearch(key);
        if (idx < 0) return null;
        return (V) values[idx];
    }

    /** Immutable list view of (key, value) pairs in byte-lex order.
     *  Each entry is a {@link Map.Entry} backed by the snapshot itself. */
    public List<Map.Entry<byte[], V>> entries() {
        List<Map.Entry<byte[], V>> out = new ArrayList<>(keys.length);
        for (int i = 0; i < keys.length; i++) {
            @SuppressWarnings("unchecked")
            V v = (V) values[i];
            out.add(Map.entry(keys[i], v));
        }
        return Collections.unmodifiableList(out);
    }

    private int binarySearch(byte[] needle) {
        int lo = 0;
        int hi = keys.length - 1;
        while (lo <= hi) {
            int mid = (lo + hi) >>> 1;
            int c = lex(keys[mid], needle);
            if (c < 0) lo = mid + 1;
            else if (c > 0) hi = mid - 1;
            else return mid;
        }
        return -1;
    }

    private static int lex(byte[] a, byte[] b) {
        int len = Math.min(a.length, b.length);
        for (int i = 0; i < len; i++) {
            int ai = a[i] & 0xff;
            int bi = b[i] & 0xff;
            if (ai != bi) return Integer.compare(ai, bi);
        }
        return Integer.compare(a.length, b.length);
    }

    /** Byte-array key view used by tests. */
    public byte[][] keysCopy() {
        byte[][] out = new byte[keys.length][];
        for (int i = 0; i < keys.length; i++) out[i] = Arrays.copyOf(keys[i], keys[i].length);
        return out;
    }
}
