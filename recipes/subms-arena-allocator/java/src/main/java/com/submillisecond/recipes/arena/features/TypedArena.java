package com.submillisecond.recipes.arena.features;

import java.util.Optional;

/**
 * Typed arena of fixed slots, with slot reuse. The Java half of Rust's
 * {@code TypedArena<T>}, and the same structure rather than an approximation
 * of one.
 *
 * <p>Allocation returns an opaque {@link Slot} handle instead of a reference.
 * Java cannot put objects inside arena memory without leaving safe Java, but an
 * index into a preallocated slot array is the same idea in both ports, and the
 * property the sub-ms claim rests on - no allocator call on the hot path -
 * holds either way. The slot array and the free stack are both sized at
 * construction, so neither {@link #alloc} nor {@link #free} allocates.
 *
 * <p>{@link #free(Slot)} pushes the slot onto a LIFO free stack and the next
 * {@link #alloc} takes it back before appending, so a workload that churns
 * fixed-shape objects inside one arena lifetime stops advancing the high-water
 * mark. {@link #reuseHits()} confirms that is happening.
 *
 * <p>Reading a slot after freeing it returns whatever the previous occupant
 * left: {@code free} does not clear the storage, and Java cannot consume the
 * handle the way Rust's by-value {@code free} does. That is a caller bug, not
 * undefined behaviour - the arena only ever reads storage it owns. Freeing the
 * same handle twice is the other half of it, and puts one index on the free
 * stack twice; the Rust port makes both cases fail to compile.
 *
 * @param <T> element type
 */
public final class TypedArena<T> {

    private final Object[] slots;
    private final int[] freeStack;
    private int freeTop;
    private int high;
    private long reuseHits;

    /**
     * New arena with room for {@code capacity} slots, floored at 1. Both the
     * slot array and the free stack are reserved up front, so no alloc
     * reallocates and no handle is invalidated by growth.
     */
    public TypedArena(int capacity) {
        int cap = Math.max(1, capacity);
        this.slots = new Object[cap];
        this.freeStack = new int[cap];
    }

    /**
     * Allocate {@code value} into a freed slot if one is available, else into a
     * fresh slot. Throws when the arena is full.
     */
    public Slot alloc(T value) {
        int idx = claim();
        if (idx < 0) {
            throw new IllegalStateException(
                "TypedArena full: capacity=" + slots.length + " len=" + len());
        }
        slots[idx] = value;
        return new Slot(idx);
    }

    /**
     * Fallible allocate. Empty when the arena is full, so the caller keeps the
     * value rather than losing it to a failed insert.
     */
    public Optional<Slot> tryAlloc(T value) {
        int idx = claim();
        if (idx < 0) {
            return Optional.empty();
        }
        slots[idx] = value;
        return Optional.of(new Slot(idx));
    }

    /** Read the value in {@code slot}. */
    @SuppressWarnings("unchecked")
    public T get(Slot slot) {
        return (T) slots[slot.index()];
    }

    /** Overwrite the value in {@code slot}. */
    public void set(Slot slot, T value) {
        slots[slot.index()] = value;
    }

    /** Return {@code slot} to the free stack for the next alloc to take. */
    public void free(Slot slot) {
        freeStack[freeTop++] = slot.index();
    }

    /**
     * Forget every slot, live or freed, and zero {@link #reuseHits()}. The slot
     * array is retained; every handle issued before the reset is stale.
     */
    public void reset() {
        freeTop = 0;
        high = 0;
        reuseHits = 0;
    }

    /** Slots currently allocated, excluding freed ones. */
    public int len() {
        return high - freeTop;
    }

    /** True when no slot is currently allocated. */
    public boolean isEmpty() {
        return len() == 0;
    }

    /** Total slots available. */
    public int capacity() {
        return slots.length;
    }

    /**
     * Allocations served from the free stack since construction or the last
     * {@link #reset()}. Confirms reuse is actually firing.
     */
    public long reuseHits() {
        return reuseHits;
    }

    /** Index of the slot to write, or -1 when the arena is full. */
    private int claim() {
        if (freeTop > 0) {
            reuseHits++;
            return freeStack[--freeTop];
        }
        if (high >= slots.length) {
            return -1;
        }
        return high++;
    }
}
