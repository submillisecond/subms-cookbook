package com.submillisecond.recipes.mergeiter.features;

import java.util.Objects;

/**
 * A keyed entry used by {@link PriorityMergeIterator}.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::PriorityEntry}.
 */
public final class PriorityEntry<K extends Comparable<K>, V> {

    private final K key;
    private final V value;

    public PriorityEntry(K key, V value) {
        this.key = Objects.requireNonNull(key, "key");
        this.value = Objects.requireNonNull(value, "value");
    }

    public K key() { return key; }
    public V value() { return value; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof PriorityEntry<?, ?> other)) return false;
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
