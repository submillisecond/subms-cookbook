package com.submillisecond.primers.perfharness;

import java.util.Arrays;
import java.util.NoSuchElementException;

/**
 * Small open-addressing long-keyed hash map. The harness is the subject of
 * this primer; the map is whatever produces realistic put / get_hit /
 * get_miss timings. Linear probing, power-of-two capacity, load factor
 * 0.75.
 *
 * <p>Not thread-safe and not feature-complete. Don't depend on this; the
 * point is the bench, not the structure.
 */
public final class TinyMap {

    private static final long EMPTY = Long.MIN_VALUE;
    private static final int  MIN_CAPACITY = 16;

    private long[] keys;
    private int[]  values;
    private int    size;
    private int    threshold;

    public TinyMap() {
        this(MIN_CAPACITY);
    }

    public TinyMap(int initialCapacity) {
        int cap = roundUpPow2(Math.max(MIN_CAPACITY, initialCapacity));
        this.keys   = new long[cap];
        this.values = new int[cap];
        Arrays.fill(keys, EMPTY);
        this.threshold = (cap * 3) >>> 2;
    }

    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }
    public int capacity() { return keys.length; }

    /** Insert or overwrite. Rejects {@link Long#MIN_VALUE} because it's the
     *  sentinel; callers wanting full 64-bit range need a different layout. */
    public void put(long key, int value) {
        if (key == EMPTY) {
            throw new IllegalArgumentException("Long.MIN_VALUE reserved as empty sentinel");
        }
        if (size >= threshold) grow();
        if (putInto(keys, values, key, value)) size++;
    }

    /** Returns the value for {@code key} or throws if absent. */
    public int get(long key) {
        int idx = indexOf(key);
        if (idx < 0) throw new NoSuchElementException("key " + key);
        return values[idx];
    }

    /** Returns true iff the key is present. */
    public boolean containsKey(long key) {
        return indexOf(key) >= 0;
    }

    /** Returns the value or {@code defaultValue} if absent. */
    public int getOrDefault(long key, int defaultValue) {
        int idx = indexOf(key);
        return idx < 0 ? defaultValue : values[idx];
    }

    /** Removes a key. Returns true iff it was present. Uses the standard
     *  open-addressing tombstone-free rehash on the displaced cluster. */
    public boolean remove(long key) {
        int idx = indexOf(key);
        if (idx < 0) return false;
        int mask = keys.length - 1;
        int next = idx;
        while (true) {
            next = (next + 1) & mask;
            long k = keys[next];
            if (k == EMPTY) {
                keys[idx] = EMPTY;
                size--;
                return true;
            }
            int desired = ((int) mix(k)) & mask;
            // Move keys whose probe would step over the freshly-emptied slot.
            if (slotInRange(desired, idx, next)) {
                keys[idx]   = k;
                values[idx] = values[next];
                idx = next;
            }
        }
    }

    private int indexOf(long key) {
        if (key == EMPTY) return -1;
        int mask = keys.length - 1;
        int i = ((int) mix(key)) & mask;
        while (true) {
            long k = keys[i];
            if (k == key)  return i;
            if (k == EMPTY) return -1;
            i = (i + 1) & mask;
        }
    }

    private void grow() {
        int newCap = keys.length << 1;
        long[] newKeys = new long[newCap];
        int[]  newVals = new int[newCap];
        Arrays.fill(newKeys, EMPTY);
        for (int i = 0; i < keys.length; i++) {
            long k = keys[i];
            if (k != EMPTY) putInto(newKeys, newVals, k, values[i]);
        }
        this.keys      = newKeys;
        this.values    = newVals;
        this.threshold = (newCap * 3) >>> 2;
    }

    private static boolean putInto(long[] ks, int[] vs, long key, int value) {
        int mask = ks.length - 1;
        int i = ((int) mix(key)) & mask;
        while (true) {
            long k = ks[i];
            if (k == EMPTY) {
                ks[i] = key;
                vs[i] = value;
                return true;
            }
            if (k == key) {
                vs[i] = value;
                return false;
            }
            i = (i + 1) & mask;
        }
    }

    /** True when {@code desired} lies in the wrap-around range
     *  {@code (after, before]} - meaning the entry currently at {@code before}
     *  ought to slide back to {@code after}'s vacated slot. */
    private static boolean slotInRange(int desired, int after, int before) {
        if (after < before) {
            return desired <= after || desired > before;
        } else {
            return desired <= after && desired > before;
        }
    }

    /** SplitMix64-style finaliser. Cheap, distributes adjacent integers well,
     *  no allocation. */
    static long mix(long k) {
        k ^= (k >>> 33);
        k *= 0xff51afd7ed558ccdL;
        k ^= (k >>> 33);
        k *= 0xc4ceb9fe1a85ec53L;
        k ^= (k >>> 33);
        return k;
    }

    private static int roundUpPow2(int x) {
        int n = 1;
        while (n < x) n <<= 1;
        return n;
    }
}
