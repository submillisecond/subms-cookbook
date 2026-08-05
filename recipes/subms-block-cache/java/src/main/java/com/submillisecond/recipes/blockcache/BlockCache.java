package com.submillisecond.recipes.blockcache;

import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;

/**
 * Clock-sweep block cache. Fixed capacity. Constant-time eviction.
 *
 * <p>Slots form a ring. Each carries a referenced bit. On insert at full
 * capacity, the hand walks the ring: a set bit becomes clear; a clear bit
 * gets evicted. Reads set the referenced bit on the hit slot.
 */
public final class BlockCache<K, V> {

    private static final class Slot<K, V> {
        K key;
        V value;
        boolean referenced;
        Slot(K k, V v) { this.key = k; this.value = v; this.referenced = true; }
    }

    public record Evicted<K, V>(K key, V value) {}

    private final int capacity;
    private final Slot<K, V>[] slots;
    private final Map<K, Integer> index;
    private int hand;
    private int size;

    @SuppressWarnings("unchecked")
    public BlockCache(int capacity) {
        int cap = Math.max(1, capacity);
        this.capacity = cap;
        this.slots = (Slot<K, V>[]) new Slot[cap];
        this.index = new HashMap<>(cap);
    }

    public int capacity() { return capacity; }
    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }

    /** Get the value for {@code key}, marking it referenced for the sweep. */
    public V get(K key) {
        Integer idx = index.get(key);
        if (idx == null) return null;
        Slot<K, V> s = slots[idx];
        s.referenced = true;
        return s.value;
    }

    /** Insert or update. Returns the evicted entry, or {@code null} if no eviction. */
    public Evicted<K, V> put(K key, V value) {
        Integer existing = index.get(key);
        if (existing != null) {
            Slot<K, V> s = slots[existing];
            s.value = value;
            s.referenced = true;
            return null;
        }
        if (size < capacity) {
            for (int i = 0; i < capacity; i++) {
                if (slots[i] == null) {
                    slots[i] = new Slot<>(key, value);
                    index.put(key, i);
                    size++;
                    return null;
                }
            }
            throw new IllegalStateException("under capacity but no empty slot");
        }
        while (true) {
            int i = hand;
            hand = (hand + 1) % capacity;
            Slot<K, V> s = slots[i];
            if (s.referenced) {
                s.referenced = false;
                continue;
            }
            Evicted<K, V> ev = new Evicted<>(s.key, s.value);
            index.remove(s.key);
            slots[i] = new Slot<>(key, value);
            index.put(key, i);
            return ev;
        }
    }

    /**
     * Invalidate {@code key}, returning its value or {@code null} if absent.
     * The vacated slot is refilled by the next insert; the hand does not move,
     * so removal costs one map lookup and one array store.
     */
    public V remove(K key) {
        Integer idx = index.remove(key);
        if (idx == null) return null;
        Slot<K, V> s = slots[idx];
        slots[idx] = null;
        size--;
        return s.value;
    }

    /** Drop every entry and reset the hand. Capacity is unchanged. */
    public void clear() {
        Arrays.fill(slots, null);
        index.clear();
        hand = 0;
        size = 0;
    }
}
