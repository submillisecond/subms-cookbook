package com.submillisecond.recipes.mergeiter.features;

import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * Descending k-way merge - the backward half of a cursor. Sources must be
 * sorted DESCENDING; output is the global descending union.
 *
 * <p>A freshly built iterator sits on the largest value across every source,
 * which is the position RocksDB calls {@code SeekToLast}.
 * {@link #seekForPrev(Comparable)} moves it to the largest value
 * {@code <= target}, and {@link #setLowerBound(Comparable)} stops the walk at
 * that value inclusive, matching RocksDB's {@code iterate_lower_bound}.
 *
 * <p>What this is NOT is a bidirectional cursor. RocksDB's {@code Prev()} can
 * reverse mid-scan because each child is a seekable file cursor; the sources
 * here are one-shot iterators that only move forward through their own order,
 * so a direction flip would have to re-read them. Pick the direction when you
 * open the merge.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::ReverseMergeIterator}.
 */
public final class ReverseMergeIterator<T extends Comparable<T>> implements Iterator<T> {

    private final List<? extends Iterator<? extends T>> streams;
    private final PriorityQueue<Entry<T>> heap;
    private T lowerBound;

    private static final class Entry<T extends Comparable<T>> {
        T value; int streamIdx;
        Entry(T v, int i) { this.value = v; this.streamIdx = i; }
    }

    public ReverseMergeIterator(List<? extends Iterator<? extends T>> streams) {
        this.streams = streams;
        // Max-heap: the largest head first, which is what a descending merge
        // wants. PriorityQueue is a min-heap, so the comparator is reversed.
        this.heap = new PriorityQueue<>(
                Math.max(1, streams.size()),
                Comparator.comparing((Entry<T> e) -> e.value).reversed());
        for (int i = 0; i < streams.size(); i++) {
            Iterator<? extends T> s = streams.get(i);
            if (s.hasNext()) heap.add(new Entry<>(s.next(), i));
        }
    }

    /**
     * Retreat past every entry strictly greater than {@code target}. After this
     * call the next {@link #next()} returns the largest value {@code <= target},
     * or the iterator is exhausted.
     */
    public void seekForPrev(T target) {
        while (!heap.isEmpty() && heap.peek().value.compareTo(target) > 0) {
            Entry<T> head = heap.poll();
            Iterator<? extends T> s = streams.get(head.streamIdx);
            T found = null;
            while (s.hasNext()) {
                T v = s.next();
                if (v.compareTo(target) <= 0) {
                    found = v;
                    break;
                }
            }
            if (found != null) heap.add(new Entry<>(found, head.streamIdx));
        }
    }

    /**
     * Stop the descending scan at {@code bound}. The bound is inclusive: a
     * value equal to it is the last one returned.
     */
    public void setLowerBound(T bound) { this.lowerBound = bound; }

    /** Drop the lower bound and let the scan run to the end of every source. */
    public void clearLowerBound() { this.lowerBound = null; }

    /**
     * The value the next {@link #next()} will return, without consuming it, or
     * {@code null} when the scan is over.
     */
    public T peek() {
        Entry<T> head = heap.peek();
        if (head == null) return null;
        if (lowerBound != null && head.value.compareTo(lowerBound) < 0) return null;
        return head.value;
    }

    /** Streams still holding a head in the heap, ignoring the lower bound. */
    public int liveStreams() { return heap.size(); }

    /** Streams the merge was constructed over, live or not. */
    public int numStreams() { return streams.size(); }

    @Override public boolean hasNext() { return peek() != null; }

    @Override public T next() {
        if (peek() == null) throw new NoSuchElementException();
        Entry<T> e = heap.poll();
        Iterator<? extends T> s = streams.get(e.streamIdx);
        if (s.hasNext()) heap.add(new Entry<>(s.next(), e.streamIdx));
        return e.value;
    }
}
