package com.submillisecond.recipes.mergeiter.features;

import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.PriorityQueue;

/**
 * Seekable k-way merge. Adds {@link #seek(Comparable)} which advances
 * the iterator past every entry strictly less than {@code target}.
 *
 * <p>After {@code seek(t)} the next call to {@link #next()} returns the
 * smallest value >= {@code t}, or throws {@link NoSuchElementException}
 * if every source is exhausted.
 *
 * <p>{@link #setUpperBound(Comparable)} closes the other end of a range scan:
 * the iterator reports exhausted once its head reaches the bound, so
 * {@code seek(lo)} plus {@code setUpperBound(hi)} walks {@code [lo, hi)} and
 * stops on its own. The bound is EXCLUSIVE, matching RocksDB's
 * {@code iterate_upper_bound}.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::SeekableMergeIterator}.
 */
public final class SeekableMergeIterator<T extends Comparable<T>> implements Iterator<T> {

    private final List<? extends Iterator<? extends T>> streams;
    private final PriorityQueue<Entry<T>> heap;
    private T upperBound;

    private static final class Entry<T extends Comparable<T>> {
        T value; int streamIdx;
        Entry(T v, int i) { this.value = v; this.streamIdx = i; }
    }

    public SeekableMergeIterator(List<? extends Iterator<? extends T>> streams) {
        this.streams = streams;
        this.heap = new PriorityQueue<>(
                Math.max(1, streams.size()),
                Comparator.comparing((Entry<T> e) -> e.value));
        for (int i = 0; i < streams.size(); i++) {
            Iterator<? extends T> s = streams.get(i);
            if (s.hasNext()) heap.add(new Entry<>(s.next(), i));
        }
    }

    /**
     * Stop the scan before {@code bound}. The bound is exclusive: a value equal
     * to it is not yielded. A bound below the current head exhausts the
     * iterator immediately.
     */
    public void setUpperBound(T bound) { this.upperBound = bound; }

    /** Drop the upper bound and let the scan run to the end of every source. */
    public void clearUpperBound() { this.upperBound = null; }

    /**
     * The value the next {@link #next()} will return, without consuming it, or
     * {@code null} when the scan is over. Respects the upper bound, so a
     * bounded scan peeks {@code null} at the same point it stops yielding.
     */
    public T peek() {
        Entry<T> head = heap.peek();
        if (head == null) return null;
        if (upperBound != null && head.value.compareTo(upperBound) >= 0) return null;
        return head.value;
    }

    /** Streams still holding a head in the heap, ignoring the upper bound. */
    public int liveStreams() { return heap.size(); }

    /** Streams the merge was constructed over, live or not. */
    public int numStreams() { return streams.size(); }

    /** Advance past every entry with key strictly less than {@code target}. */
    public void seek(T target) {
        // Pop heads < target, walk those streams forward to the first
        // value >= target (or exhaustion), then push that head back.
        while (!heap.isEmpty() && heap.peek().value.compareTo(target) < 0) {
            Entry<T> head = heap.poll();
            Iterator<? extends T> s = streams.get(head.streamIdx);
            T found = null;
            while (s.hasNext()) {
                T v = s.next();
                if (v.compareTo(target) >= 0) {
                    found = v;
                    break;
                }
            }
            if (found != null) heap.add(new Entry<>(found, head.streamIdx));
        }
    }

    @Override public boolean hasNext() { return peek() != null; }

    @Override public T next() {
        if (peek() == null) throw new NoSuchElementException();
        Entry<T> e = heap.poll();
        Iterator<? extends T> s = streams.get(e.streamIdx);
        if (s.hasNext()) heap.add(new Entry<>(s.next(), e.streamIdx));
        return e.value;
    }
}
