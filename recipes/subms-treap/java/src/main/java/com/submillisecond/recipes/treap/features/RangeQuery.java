package com.submillisecond.recipes.treap.features;

import com.submillisecond.recipes.treap.Treap;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;

/**
 * Sorted-range iteration over a treap.
 *
 * <p>Builds a frozen sorted snapshot of the source treap, then yields
 * every {@code (K, V)} entry whose key falls between the bounds in
 * ascending order. Bounds are inclusive or exclusive; mix freely.
 *
 * <p>The snapshot semantics give the "stable iteration even under
 * concurrent reads" guarantee: once a {@code RangeQuery} is built,
 * subsequent mutations on the source treap don't affect held iterators.
 *
 * <p>Byte-equivalent to the Rust sibling {@code subms_treap::RangeIter}.
 */
public final class RangeQuery<K extends Comparable<K>, V> implements Iterable<Map.Entry<K, V>> {

    private final List<Map.Entry<K, V>> window;

    private RangeQuery(List<Map.Entry<K, V>> window) {
        this.window = window;
    }

    /**
     * Build a range query over {@code treap}. {@code from} and {@code to}
     * may be {@code null} to denote unbounded ends. The booleans pick
     * inclusive ({@code true}) or exclusive ({@code false}) semantics
     * at each end.
     */
    public static <K extends Comparable<K>, V> RangeQuery<K, V> of(
            Treap<K, V> treap, K from, boolean fromInclusive, K to, boolean toInclusive) {
        // Bounded descent in the treap (O(log n + window)); the snapshot is
        // a copy of just the window, not the whole tree. Stable under later
        // mutation of the source - see the class doc.
        return new RangeQuery<>(treap.collectRange(from, fromInclusive, to, toInclusive));
    }

    public int size() {
        return window.size();
    }

    public boolean isEmpty() {
        return window.isEmpty();
    }

    /** Snapshot of the windowed entries in sorted order. */
    public List<Map.Entry<K, V>> toList() {
        return new ArrayList<>(window);
    }

    @Override
    public Iterator<Map.Entry<K, V>> iterator() {
        return new Iterator<>() {
            private int i = 0;

            @Override
            public boolean hasNext() {
                return i < window.size();
            }

            @Override
            public Map.Entry<K, V> next() {
                if (i >= window.size()) throw new NoSuchElementException();
                return window.get(i++);
            }
        };
    }
}
