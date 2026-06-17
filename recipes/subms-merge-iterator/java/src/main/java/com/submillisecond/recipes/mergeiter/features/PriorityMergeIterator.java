package com.submillisecond.recipes.mergeiter.features;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * Priority-aware k-way merge. Each source carries an explicit
 * {@code priority}. On key tie, the highest-priority source wins.
 * Equal-priority ties fall through to the source's registration
 * order (higher source index wins).
 *
 * <p>Generalises {@link DedupMergeIterator}: pass {@code priority =
 * sourceIndex} for the same behaviour. Reach for this when on-the-wire
 * source order does not match the authority order you want.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::PriorityMergeIterator}.
 */
public final class PriorityMergeIterator<K extends Comparable<K>, V>
        implements Iterator<PriorityEntry<K, V>> {

    private final List<Iterator<? extends PriorityEntry<K, V>>> streams;
    private final int[] priorities;
    private final PriorityQueue<HeapItem<K, V>> heap;

    private static final class HeapItem<K extends Comparable<K>, V> {
        K key;
        int priority;
        int source;
        V value;
        HeapItem(K k, int p, int s, V v) { this.key = k; this.priority = p; this.source = s; this.value = v; }
    }

    public PriorityMergeIterator(List<? extends PrioritySource<K, V>> sources) {
        this.streams = new ArrayList<>(sources.size());
        this.priorities = new int[sources.size()];
        this.heap = new PriorityQueue<>(Math.max(1, sources.size()), (a, b) -> {
            int byKey = a.key.compareTo(b.key);
            if (byKey != 0) return byKey;
            // Higher priority pops first.
            int byPrio = Integer.compare(b.priority, a.priority);
            if (byPrio != 0) return byPrio;
            // Latest source wins on priority tie.
            return Integer.compare(b.source, a.source);
        });
        for (int i = 0; i < sources.size(); i++) {
            PrioritySource<K, V> src = sources.get(i);
            this.priorities[i] = src.priority();
            this.streams.add(src.stream());
            Iterator<? extends PriorityEntry<K, V>> s = src.stream();
            if (s.hasNext()) {
                PriorityEntry<K, V> e = s.next();
                heap.add(new HeapItem<>(e.key(), src.priority(), i, e.value()));
            }
        }
    }

    @Override public boolean hasNext() { return !heap.isEmpty(); }

    @Override public PriorityEntry<K, V> next() {
        HeapItem<K, V> winner = heap.poll();
        if (winner == null) throw new NoSuchElementException();
        K winningKey = winner.key;
        V winningValue = winner.value;
        advanceSource(winner.source);
        while (!heap.isEmpty() && heap.peek().key.compareTo(winningKey) == 0) {
            HeapItem<K, V> dup = heap.poll();
            advanceSource(dup.source);
        }
        return new PriorityEntry<>(winningKey, winningValue);
    }

    private void advanceSource(int source) {
        Iterator<? extends PriorityEntry<K, V>> s = streams.get(source);
        if (s.hasNext()) {
            PriorityEntry<K, V> e = s.next();
            heap.add(new HeapItem<>(e.key(), priorities[source], source, e.value()));
        }
    }
}
