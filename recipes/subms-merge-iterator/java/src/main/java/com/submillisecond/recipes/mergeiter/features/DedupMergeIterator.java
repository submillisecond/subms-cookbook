package com.submillisecond.recipes.mergeiter.features;

import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * Deduplicating k-way merge. Each input is a stream of (key, value)
 * entries sorted by key. On key tie across sources, the highest-
 * indexed source wins. Output is one entry per distinct key.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::DedupMergeIterator}.
 */
public final class DedupMergeIterator<K extends Comparable<K>, V>
        implements Iterator<DedupEntry<K, V>> {

    private final List<? extends Iterator<? extends DedupEntry<K, V>>> streams;
    private final PriorityQueue<HeapItem<K, V>> heap;

    private static final class HeapItem<K extends Comparable<K>, V> {
        K key;
        int source;
        V value;
        HeapItem(K k, int s, V v) { this.key = k; this.source = s; this.value = v; }
    }

    public DedupMergeIterator(List<? extends Iterator<? extends DedupEntry<K, V>>> streams) {
        this.streams = streams;
        this.heap = new PriorityQueue<>(Math.max(1, streams.size()), (a, b) -> {
            int byKey = a.key.compareTo(b.key);
            if (byKey != 0) return byKey;
            return Integer.compare(b.source, a.source);
        });
        for (int i = 0; i < streams.size(); i++) {
            Iterator<? extends DedupEntry<K, V>> s = streams.get(i);
            if (s.hasNext()) {
                DedupEntry<K, V> e = s.next();
                heap.add(new HeapItem<>(e.key(), i, e.value()));
            }
        }
    }

    @Override public boolean hasNext() { return !heap.isEmpty(); }

    @Override public DedupEntry<K, V> next() {
        HeapItem<K, V> winner = heap.poll();
        if (winner == null) throw new NoSuchElementException();
        K winningKey = winner.key;
        V winningValue = winner.value;
        advanceSource(winner.source);
        while (!heap.isEmpty() && heap.peek().key.compareTo(winningKey) == 0) {
            HeapItem<K, V> dup = heap.poll();
            advanceSource(dup.source);
        }
        return new DedupEntry<>(winningKey, winningValue);
    }

    private void advanceSource(int source) {
        Iterator<? extends DedupEntry<K, V>> s = streams.get(source);
        if (s.hasNext()) {
            DedupEntry<K, V> e = s.next();
            heap.add(new HeapItem<>(e.key(), source, e.value()));
        }
    }
}
