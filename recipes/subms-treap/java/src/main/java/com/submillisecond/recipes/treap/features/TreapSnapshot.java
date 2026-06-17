package com.submillisecond.recipes.treap.features;

import com.submillisecond.recipes.treap.Treap;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * Read-only frozen snapshot of a treap, safe to share across reader
 * threads while writers mutate the source.
 *
 * <p>Built from an immutable {@code List<Map.Entry<K, V>>} sorted by
 * key. Readers receive a shared reference to the same backing list
 * (cheap, no copy), and the source treap can continue taking
 * inserts / deletes without disturbing held snapshots.
 *
 * <p>Java equivalent of the Rust {@code Arc<Inner>} sibling: an
 * immutable wrapper that holds its own root reference and is safe
 * to publish to other threads. Acts as a thread-safe read view via
 * the immutability guarantee of {@link Collections#unmodifiableList(List)}.
 *
 * <p>Composition: combine with {@link RangeQuery} via
 * {@link #range(Comparable, Comparable)} for sorted-range iteration
 * over the snapshot, or take fresh snapshots periodically as the
 * source treap evolves.
 *
 * <p>Byte-equivalent to the Rust sibling {@code subms_treap::TreapSnapshot}.
 */
public final class TreapSnapshot<K extends Comparable<K>, V> implements Iterable<Map.Entry<K, V>> {

    /** Sorted by key. Frozen (unmodifiable view), safe to share. */
    private final List<Map.Entry<K, V>> data;

    private TreapSnapshot(List<Map.Entry<K, V>> data) {
        this.data = data;
    }

    /**
     * Build a frozen snapshot from the current state of {@code treap}.
     * The result is safe to publish to other threads; mutations on
     * the source treap after this call don't affect it.
     */
    public static <K extends Comparable<K>, V> TreapSnapshot<K, V> fromTreap(Treap<K, V> treap) {
        List<Map.Entry<K, V>> data = new ArrayList<>(treap.collectEntriesInOrder());
        return new TreapSnapshot<>(Collections.unmodifiableList(data));
    }

    public int size() { return data.size(); }
    public boolean isEmpty() { return data.isEmpty(); }

    /** Binary-search lookup over the sorted snapshot. */
    public V get(K key) {
        int lo = 0;
        int hi = data.size() - 1;
        while (lo <= hi) {
            int mid = (lo + hi) >>> 1;
            int cmp = data.get(mid).getKey().compareTo(key);
            if (cmp == 0) return data.get(mid).getValue();
            if (cmp < 0) lo = mid + 1;
            else hi = mid - 1;
        }
        return null;
    }

    /** Sorted view of every {@code (K, V)} in the snapshot. */
    public List<Map.Entry<K, V>> entries() {
        return data;
    }

    @Override
    public Iterator<Map.Entry<K, V>> iterator() {
        return data.iterator();
    }

    /** Sorted range over {@code [from, to]} (both inclusive). */
    public List<Map.Entry<K, V>> range(K from, K to) {
        int loIdx = lowerBound(from);
        int hiIdx = upperBound(to);
        if (loIdx >= hiIdx) return Collections.emptyList();
        return data.subList(loIdx, hiIdx);
    }

    /** Index of first entry with {@code key >= target}. */
    private int lowerBound(K target) {
        int lo = 0;
        int hi = data.size();
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (data.get(mid).getKey().compareTo(target) < 0) lo = mid + 1;
            else hi = mid;
        }
        return lo;
    }

    /** Index of first entry with {@code key > target}. */
    private int upperBound(K target) {
        int lo = 0;
        int hi = data.size();
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (data.get(mid).getKey().compareTo(target) <= 0) lo = mid + 1;
            else hi = mid;
        }
        return lo;
    }
}
