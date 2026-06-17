package com.submillisecond.recipes.mergeiter.features;

import java.util.Objects;

/**
 * A keyed entry. {@code value} is {@code null} for a tombstone
 * (deletion marker) and non-null for a live entry.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::TombstoneEntry}.
 */
public final class TombstoneEntry<K extends Comparable<K>, V> {

    private final K key;
    private final V value;

    private TombstoneEntry(K key, V value) {
        this.key = key;
        this.value = value;
    }

    public static <K extends Comparable<K>, V> TombstoneEntry<K, V> live(K key, V value) {
        if (value == null) {
            throw new IllegalArgumentException("live value must not be null - use tombstone() instead");
        }
        return new TombstoneEntry<>(key, value);
    }

    public static <K extends Comparable<K>, V> TombstoneEntry<K, V> tombstone(K key) {
        return new TombstoneEntry<>(key, null);
    }

    public K key() { return key; }
    public V value() { return value; }
    public boolean isTombstone() { return value == null; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof TombstoneEntry<?, ?> other)) return false;
        return Objects.equals(key, other.key) && Objects.equals(value, other.value);
    }

    @Override
    public int hashCode() {
        return Objects.hash(key, value);
    }

    @Override
    public String toString() {
        return value == null ? "Tomb(" + key + ")" : "Live(" + key + "=" + value + ")";
    }
}
