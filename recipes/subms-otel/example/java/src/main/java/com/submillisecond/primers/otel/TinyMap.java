package com.submillisecond.primers.otel;

/**
 * Open-addressing long-keyed map with linear probing. Small enough to read in
 * one sitting; just enough public surface to drive a realistic {@code put} /
 * {@code get_hit} / {@code get_miss} workload through the harness.
 *
 * <p>Not a recipe-grade implementation. The point of this primer is the
 * observer hook, not the data structure: the map is here so the workload has
 * something to time.
 */
public final class TinyMap {

    private static final long EMPTY = 0L;
    private static final double LOAD_FACTOR = 0.75;

    private long[] keys;
    private long[] values;
    private int size;
    private int threshold;

    public TinyMap() {
        this(16);
    }

    public TinyMap(int initialCapacity) {
        if (initialCapacity <= 0) throw new IllegalArgumentException("initialCapacity must be > 0");
        int cap = nextPowerOfTwo(Math.max(initialCapacity, 8));
        this.keys = new long[cap];
        this.values = new long[cap];
        this.threshold = (int) (cap * LOAD_FACTOR);
    }

    /** Insert or overwrite. Key 0 is reserved as the empty marker; rejected up front. */
    public void put(long key, long value) {
        if (key == EMPTY) throw new IllegalArgumentException("key 0 is reserved");
        if (size >= threshold) grow();
        int mask = keys.length - 1;
        int i = ((int) (mix(key))) & mask;
        while (true) {
            long k = keys[i];
            if (k == EMPTY) {
                keys[i] = key;
                values[i] = value;
                size++;
                return;
            }
            if (k == key) {
                values[i] = value;
                return;
            }
            i = (i + 1) & mask;
        }
    }

    /** Returns {@link Long#MIN_VALUE} on miss to keep the hot path branch-free. */
    public long get(long key) {
        if (key == EMPTY) return Long.MIN_VALUE;
        int mask = keys.length - 1;
        int i = ((int) mix(key)) & mask;
        while (true) {
            long k = keys[i];
            if (k == EMPTY) return Long.MIN_VALUE;
            if (k == key) return values[i];
            i = (i + 1) & mask;
        }
    }

    public int size() {
        return size;
    }

    public int capacity() {
        return keys.length;
    }

    private void grow() {
        long[] oldKeys = keys;
        long[] oldValues = values;
        int newCap = oldKeys.length * 2;
        this.keys = new long[newCap];
        this.values = new long[newCap];
        this.size = 0;
        this.threshold = (int) (newCap * LOAD_FACTOR);
        for (int i = 0; i < oldKeys.length; i++) {
            long k = oldKeys[i];
            if (k != EMPTY) put(k, oldValues[i]);
        }
    }

    private static int nextPowerOfTwo(int n) {
        int v = n - 1;
        v |= v >>> 1;
        v |= v >>> 2;
        v |= v >>> 4;
        v |= v >>> 8;
        v |= v >>> 16;
        return v + 1;
    }

    // splitmix64 finaliser - keeps neighbour keys from clustering under linear probing.
    private static long mix(long x) {
        x ^= (x >>> 30);
        x *= 0xbf58476d1ce4e5b7L;
        x ^= (x >>> 27);
        x *= 0x94d049bb133111ebL;
        x ^= (x >>> 31);
        return x;
    }
}
