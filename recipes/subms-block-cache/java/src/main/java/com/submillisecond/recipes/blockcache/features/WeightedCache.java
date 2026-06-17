package com.submillisecond.recipes.blockcache.features;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import java.util.function.ToIntFunction;

/**
 * Weighted block cache: per-entry byte size.
 *
 * <p>Counts capacity in bytes, not slots. The caller supplies a
 * {@code sizeOf} function. {@code put} adjusts a running total; if
 * the total exceeds {@code capacityBytes}, evicts via clock-sweep
 * until it fits. Eviction may free more than the minimum required.
 *
 * <p>A single put whose value is larger than the entire capacity is
 * rejected: the new entry is returned as the evicted pair without
 * touching any resident.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_block_cache::features::weighted::WeightedCache}.
 */
public final class WeightedCache<K, V> {

    private static final class Slot<K, V> {
        K key;
        V value;
        int size;
        boolean referenced;
        Slot(K k, V v, int s) { this.key = k; this.value = v; this.size = s; this.referenced = true; }
    }

    public record Evicted<K, V>(K key, V value) {}

    private final int capacityBytes;
    private int usedBytes;
    private final ArrayList<Slot<K, V>> slots = new ArrayList<>();
    private final Map<K, Integer> index = new HashMap<>();
    private int hand;
    private final ToIntFunction<V> sizeOf;

    public WeightedCache(int capacityBytes, ToIntFunction<V> sizeOf) {
        this.capacityBytes = Math.max(1, capacityBytes);
        this.sizeOf = sizeOf;
    }

    public int capacityBytes() { return capacityBytes; }
    public int usedBytes() { return usedBytes; }
    public int size() { return index.size(); }
    public boolean isEmpty() { return index.isEmpty(); }

    public V get(K key) {
        Integer boxed = index.get(key);
        if (boxed == null) return null;
        Slot<K, V> s = slots.get(boxed);
        s.referenced = true;
        return s.value;
    }

    /** Insert or update. Returns the list of evictions (possibly empty). */
    public java.util.List<Evicted<K, V>> put(K key, V value) {
        int newSize = sizeOf.applyAsInt(value);
        if (newSize > capacityBytes) {
            return java.util.List.of(new Evicted<>(key, value));
        }

        Integer existing = index.get(key);
        if (existing != null) {
            int id = existing;
            Slot<K, V> s = slots.get(id);
            int oldSize = s.size;
            s.value = value;
            s.referenced = true;
            s.size = newSize;
            usedBytes = usedBytes + newSize - oldSize;
            java.util.List<Evicted<K, V>> ev = new java.util.ArrayList<>();
            while (usedBytes > capacityBytes) {
                Evicted<K, V> e = sweepEvictExcluding(id);
                if (e == null) break;
                ev.add(e);
            }
            return ev;
        }

        java.util.List<Evicted<K, V>> evicted = new java.util.ArrayList<>();
        while (usedBytes + newSize > capacityBytes) {
            Evicted<K, V> e = sweepEvictExcluding(-1);
            if (e == null) break;
            evicted.add(e);
        }

        int newId = findFreeSlot();
        Slot<K, V> newSlot = new Slot<>(key, value, newSize);
        if (newId == slots.size()) {
            slots.add(newSlot);
        } else {
            slots.set(newId, newSlot);
        }
        index.put(key, newId);
        usedBytes += newSize;
        return evicted;
    }

    private int findFreeSlot() {
        for (int i = 0; i < slots.size(); i++) {
            if (slots.get(i) == null) return i;
        }
        return slots.size();
    }

    private Evicted<K, V> sweepEvictExcluding(int skip) {
        if (index.isEmpty()) return null;
        int n = slots.size();
        if (n == 0) return null;
        // Sweep terminates: every populated, non-skipped slot has its
        // ref bit cleared on first visit; second visit always evicts.
        // Total visits bounded by 2n + 1.
        for (int visits = 0; visits < 2 * n + 1; visits++) {
            int i = ((hand % n) + n) % n;
            hand = (hand + 1) % Math.max(1, n);
            Slot<K, V> s = slots.get(i);
            if (s == null) continue;
            if (i == skip) continue;
            if (s.referenced) {
                s.referenced = false;
                continue;
            }
            slots.set(i, null);
            index.remove(s.key);
            usedBytes -= s.size;
            return new Evicted<>(s.key, s.value);
        }
        return null;
    }
}
