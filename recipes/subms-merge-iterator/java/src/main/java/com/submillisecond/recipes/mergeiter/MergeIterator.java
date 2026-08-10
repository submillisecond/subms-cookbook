package com.submillisecond.recipes.mergeiter;

import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * K-way merge of sorted iterators. Min-heap over (current value, stream index).
 * Pop the head; advance that stream; push its next value if any.
 *
 * <p>All input iterators must produce ascending sequences (per the natural
 * order of {@code T}). Output is the global ascending union.
 *
 * <p>Not thread-safe. It is a single-threaded cursor over sources it advances
 * itself: one merge per consumer.
 */
public final class MergeIterator<T extends Comparable<T>> implements Iterator<T> {

    private final List<? extends Iterator<? extends T>> streams;
    private final PriorityQueue<Entry<T>> heap;

    private static final class Entry<T extends Comparable<T>> {
        T value; int streamIdx;
        Entry(T v, int i) { this.value = v; this.streamIdx = i; }
    }

    public MergeIterator(List<? extends Iterator<? extends T>> streams) {
        this.streams = streams;
        this.heap = new PriorityQueue<>(Math.max(1, streams.size()),
                Comparator.comparing((Entry<T> e) -> e.value));
        for (int i = 0; i < streams.size(); i++) {
            Iterator<? extends T> s = streams.get(i);
            if (s.hasNext()) heap.add(new Entry<>(s.next(), i));
        }
    }

    /**
     * The value the next {@link #next()} will return, without consuming it.
     * {@code null} when the merge is exhausted.
     */
    public T peek() {
        Entry<T> head = heap.peek();
        return head == null ? null : head.value;
    }

    /**
     * Streams still holding a head in the heap. Drops to zero exactly when the
     * merge is exhausted, so this doubles as the RocksDB-style {@code valid()}
     * check.
     */
    public int liveStreams() { return heap.size(); }

    /** Streams the merge was constructed over, live or not. */
    public int numStreams() { return streams.size(); }

    @Override public boolean hasNext() { return !heap.isEmpty(); }

    @Override public T next() {
        Entry<T> e = heap.poll();
        if (e == null) throw new NoSuchElementException();
        Iterator<? extends T> s = streams.get(e.streamIdx);
        if (s.hasNext()) heap.add(new Entry<>(s.next(), e.streamIdx));
        return e.value;
    }
}
