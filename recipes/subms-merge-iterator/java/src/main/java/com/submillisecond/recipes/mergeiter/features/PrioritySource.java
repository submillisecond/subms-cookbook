package com.submillisecond.recipes.mergeiter.features;

import java.util.Iterator;
import java.util.Objects;

/**
 * A source + the priority it carries. Higher {@code priority} wins on
 * key ties inside {@link PriorityMergeIterator}. Ties between equal
 * priorities fall back to registration order (higher source index
 * wins).
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::PrioritySource}.
 */
public final class PrioritySource<K extends Comparable<K>, V> {

    private final int priority;
    private final Iterator<? extends PriorityEntry<K, V>> stream;

    public PrioritySource(int priority, Iterator<? extends PriorityEntry<K, V>> stream) {
        this.priority = priority;
        this.stream = Objects.requireNonNull(stream, "stream");
    }

    public int priority() { return priority; }
    public Iterator<? extends PriorityEntry<K, V>> stream() { return stream; }
}
