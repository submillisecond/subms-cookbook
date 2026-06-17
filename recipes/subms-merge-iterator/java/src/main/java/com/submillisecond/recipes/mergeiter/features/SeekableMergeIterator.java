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
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_merge_iterator::SeekableMergeIterator}.
 */
public final class SeekableMergeIterator<T extends Comparable<T>> implements Iterator<T> {

    private final List<? extends Iterator<? extends T>> streams;
    private final PriorityQueue<Entry<T>> heap;

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

    @Override public boolean hasNext() { return !heap.isEmpty(); }

    @Override public T next() {
        Entry<T> e = heap.poll();
        if (e == null) throw new NoSuchElementException();
        Iterator<? extends T> s = streams.get(e.streamIdx);
        if (s.hasNext()) heap.add(new Entry<>(s.next(), e.streamIdx));
        return e.value;
    }
}
