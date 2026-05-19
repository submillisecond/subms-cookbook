package com.submillisecond.recipes.lsm;

import java.util.Optional;
import java.util.TreeMap;

/**
 * In-memory buffer of pending writes, sorted by key. A {@code null} value
 * represents a tombstone - present in the map, marking the key as deleted.
 */
final class Memtable {

    private final TreeMap<String, byte[]> entries = new TreeMap<>();
    private int approxSizeBytes = 0;

    void put(String key, byte[] value) {
        byte[] prev = entries.put(key, value);
        if (prev == null) {
            approxSizeBytes += keyCost(key) + valueCost(value);
        } else {
            approxSizeBytes += valueCost(value) - valueCost(prev);
        }
    }

    void delete(String key) {
        put(key, null);
    }

    /**
     * @return {@link Optional#empty()} if the key is not in the memtable at all;
     *         an empty {@link Lookup} if the key is tombstoned;
     *         a {@link Lookup} carrying the value otherwise.
     */
    Optional<Lookup> get(String key) {
        if (!entries.containsKey(key)) return Optional.empty();
        return Optional.of(new Lookup(entries.get(key)));
    }

    int approxSizeBytes() {
        return approxSizeBytes;
    }

    int entryCount() {
        return entries.size();
    }

    boolean isEmpty() {
        return entries.isEmpty();
    }

    Iterable<java.util.Map.Entry<String, byte[]>> sortedEntries() {
        return entries.entrySet();
    }

    void clear() {
        entries.clear();
        approxSizeBytes = 0;
    }

    private static int keyCost(String key) {
        return key == null ? 0 : key.length();
    }

    private static int valueCost(byte[] value) {
        return value == null ? 1 : value.length;
    }

    record Lookup(byte[] value) {
        boolean isTombstone() {
            return value == null;
        }
    }
}
