package com.submillisecond.recipes.mergeiter.features;

import java.util.Objects;

/**
 * A keyed entry. Both fields are non-null.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::DedupEntry}.
 */
public final class DedupEntry<K extends Comparable<K>, V> {

    private final K key;
    private final V value;

    public DedupEntry(K key, V value) {
        this.key = Objects.requireNonNull(key, "key");
        this.value = Objects.requireNonNull(value, "value");
    }

    public K key() { return key; }
    public V value() { return value; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof DedupEntry<?, ?> other)) return false;
        return key.equals(other.key) && value.equals(other.value);
    }

    @Override
    public int hashCode() {
        return Objects.hash(key, value);
    }

    @Override
    public String toString() {
        return "(" + key + "=" + value + ")";
    }
}
