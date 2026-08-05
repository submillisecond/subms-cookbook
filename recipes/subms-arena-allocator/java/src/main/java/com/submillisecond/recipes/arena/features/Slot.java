package com.submillisecond.recipes.arena.features;

/**
 * Opaque handle to one slot in a {@link TypedArena}. Opaque so a caller cannot
 * do arithmetic into a slot it never allocated: the index is readable through
 * {@link #index()} but there is no public way to build a handle from one.
 *
 * <p>A handle carries no arena identity. Passing a slot from one arena to
 * another reads that arena's storage at the same index.
 */
public final class Slot {

    private final int index;

    Slot(int index) {
        this.index = index;
    }

    /** Position of the slot in the arena's storage. */
    public int index() {
        return index;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Slot other && other.index == index;
    }

    @Override
    public int hashCode() {
        return index;
    }

    @Override
    public String toString() {
        return "Slot(" + index + ")";
    }
}
