package com.submillisecond.recipes.arena.features;

import java.util.function.Supplier;

/**
 * Typed object pool that mirrors Rust's {@code TypedArena<T>}. Parametric
 * over a single type; {@link #allocate()} returns an instance constructed
 * by the supplied factory the first time a slot is needed, or recycled
 * via {@link #reset()} on subsequent rounds.
 *
 * <p>Java has no raw bump allocator over heap objects - the GC owns
 * placement. The pool shape gives the same allocate/reset semantics: the
 * pool holds at most {@code capacity} instances, hands them out in
 * order, and {@link #reset()} marks them all reusable without releasing
 * the references.
 *
 * <p>Allocated objects are NOT re-initialised on recycle. The caller is
 * expected to mutate the returned object's state before each use (the
 * typical object-pool contract).
 *
 * @param <T> element type
 */
public final class TypedArena<T> {

    private final Object[] slots;
    private final Supplier<T> factory;
    private int len;

    public TypedArena(int capacity, Supplier<T> factory) {
        if (capacity < 1) throw new IllegalArgumentException("capacity >= 1 required: " + capacity);
        if (factory == null) throw new NullPointerException("factory");
        this.slots = new Object[capacity];
        this.factory = factory;
        this.len = 0;
    }

    /** Allocate the next slot. Throws if the arena is full. */
    @SuppressWarnings("unchecked")
    public T allocate() {
        if (len >= slots.length) {
            throw new IllegalStateException(
                "TypedArena full: capacity=" + slots.length + " len=" + len);
        }
        Object existing = slots[len];
        if (existing == null) {
            existing = factory.get();
            slots[len] = existing;
        }
        len++;
        return (T) existing;
    }

    /** Mark every previously-allocated object as reusable. The objects
     *  themselves are retained for the next round. */
    public void reset() {
        len = 0;
    }

    public int len() {
        return len;
    }

    public int capacity() {
        return slots.length;
    }

    /** True iff no slot is currently in-use. */
    public boolean isEmpty() {
        return len == 0;
    }
}
