package com.submillisecond.recipes.arena.features;

import java.util.ArrayDeque;
import java.util.Deque;
import java.util.function.Supplier;

/**
 * Typed object pool with explicit release/reuse semantics.
 *
 * <p>The Java parallel of Rust's {@code FreelistBump}: the pool holds a
 * LIFO bucket of returned objects. {@link #allocate()} pops from the
 * bucket if non-empty, else constructs a new instance via the factory
 * (subject to the configured capacity). {@link #release(Object)} returns
 * an object to the bucket for reuse.
 *
 * <p>Trade-off vs {@link TypedArena}: this variant lets callers free
 * individual objects out-of-order; reuse comes back from the bucket
 * before the cursor advances. Workloads that retain a large but
 * fluctuating live set benefit; pure allocate-then-reset workloads
 * should prefer {@link TypedArena}.
 *
 * <p>Allocated objects are NOT re-initialised on reuse. The caller is
 * expected to overwrite the object's state per the standard pool
 * contract.
 *
 * @param <T> element type
 */
public final class FreelistArena<T> {

    private final Supplier<T> factory;
    private final int capacity;
    private int issued;
    private long reuseHits;
    private final Deque<T> bucket = new ArrayDeque<>();

    public FreelistArena(int capacity, Supplier<T> factory) {
        if (capacity < 1) throw new IllegalArgumentException("capacity >= 1: " + capacity);
        if (factory == null) throw new NullPointerException("factory");
        this.capacity = capacity;
        this.factory = factory;
    }

    /**
     * Pop from the freelist if non-empty, otherwise construct a fresh
     * instance via the factory. Throws when the pool would exceed
     * capacity (i.e. {@code issued >= capacity} and the bucket is
     * empty).
     */
    public T allocate() {
        T pooled = bucket.pollFirst();
        if (pooled != null) {
            reuseHits++;
            return pooled;
        }
        if (issued >= capacity) {
            throw new IllegalStateException(
                "FreelistArena exhausted: capacity=" + capacity + " issued=" + issued);
        }
        issued++;
        return factory.get();
    }

    /** Return {@code obj} to the freelist for reuse. */
    public void release(T obj) {
        if (obj == null) throw new NullPointerException("obj");
        bucket.addFirst(obj);
    }

    /** Drop every freelist entry and zero the issued counter. */
    public void reset() {
        bucket.clear();
        issued = 0;
        reuseHits = 0;
    }

    public int capacity() {
        return capacity;
    }

    public int issued() {
        return issued;
    }

    public int freelistLen() {
        return bucket.size();
    }

    public long reuseHits() {
        return reuseHits;
    }
}
