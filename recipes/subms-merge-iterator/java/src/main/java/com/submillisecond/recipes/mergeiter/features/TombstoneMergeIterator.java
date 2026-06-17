package com.submillisecond.recipes.mergeiter.features;

import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * Tombstone-aware k-way merge. Entries carry a key and an optional
 * value; a {@code null} value marks the key as deleted.
 *
 * <p>On key tie across sources, the highest-indexed source ("latest")
 * wins. If the winning entry is a tombstone, the key is dropped from
 * the output; otherwise it is yielded.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::TombstoneMergeIterator}.
 */
public final class TombstoneMergeIterator<K extends Comparable<K>, V>
        implements Iterator<TombstoneEntry<K, V>> {

    private final List<? extends Iterator<? extends TombstoneEntry<K, V>>> streams;
    private final PriorityQueue<HeapItem<K, V>> heap;
    private TombstoneEntry<K, V> nextLive;

    private static final class HeapItem<K extends Comparable<K>, V> {
        K key;
        int source;
        V value;
        HeapItem(K k, int s, V v) { this.key = k; this.source = s; this.value = v; }
    }

    public TombstoneMergeIterator(List<? extends Iterator<? extends TombstoneEntry<K, V>>> streams) {
        this.streams = streams;
        this.heap = new PriorityQueue<>(Math.max(1, streams.size()), (a, b) -> {
            int byKey = a.key.compareTo(b.key);
            if (byKey != 0) return byKey;
            // Equal keys: higher source first (so the latest wins the
            // pop). Comparator returns negative when `a` should pop
            // before `b`.
            return Integer.compare(b.source, a.source);
        });
        for (int i = 0; i < streams.size(); i++) {
            Iterator<? extends TombstoneEntry<K, V>> s = streams.get(i);
            if (s.hasNext()) {
                TombstoneEntry<K, V> e = s.next();
                heap.add(new HeapItem<>(e.key(), i, e.value()));
            }
        }
        advanceToNextLive();
    }

    @Override public boolean hasNext() { return nextLive != null; }

    @Override public TombstoneEntry<K, V> next() {
        if (nextLive == null) throw new NoSuchElementException();
        TombstoneEntry<K, V> out = nextLive;
        advanceToNextLive();
        return out;
    }

    private void advanceToNextLive() {
        while (!heap.isEmpty()) {
            HeapItem<K, V> winner = heap.poll();
            K winningKey = winner.key;
            V winningValue = winner.value;
            advanceSource(winner.source);
            // Drop every other entry that shares the winning key.
            while (!heap.isEmpty() && heap.peek().key.compareTo(winningKey) == 0) {
                HeapItem<K, V> dup = heap.poll();
                advanceSource(dup.source);
            }
            if (winningValue != null) {
                nextLive = TombstoneEntry.live(winningKey, winningValue);
                return;
            }
            // Tombstone wins - loop to find the next distinct live key.
        }
        nextLive = null;
    }

    private void advanceSource(int source) {
        Iterator<? extends TombstoneEntry<K, V>> s = streams.get(source);
        if (s.hasNext()) {
            TombstoneEntry<K, V> e = s.next();
            heap.add(new HeapItem<>(e.key(), source, e.value()));
        }
    }
}
