package com.submillisecond.recipes.arena;

/**
 * Bump-pointer arena over a {@code byte[]}. {@link #allocate(int, int)}
 * returns an offset into the backing buffer aligned to the requested
 * alignment; {@link #reset()} rewinds the bump cursor without freeing the
 * buffer.
 *
 * <p>Chunked growth: if a request would overflow the current buffer, a fresh
 * one is allocated at twice the previous size (or the request size, whichever
 * is larger). On {@link #reset()} only the largest chunk is retained.
 *
 * <p>Caller writes through {@link #bytes()} at the returned offset.
 */
public final class BumpArena {

    private byte[] bytes;
    private int cursor;
    /** Total bytes ever allocated across the chunk history; survives {@link #reset()}. */
    private int totalCapacity;

    public BumpArena(int initialBytes) {
        int cap = Math.max(64, initialBytes);
        this.bytes = new byte[cap];
        this.totalCapacity = cap;
    }

    /** Return a byte offset aligned to {@code align}, with {@code size} bytes available from it. */
    public int allocate(int size, int align) {
        if ((align & (align - 1)) != 0) throw new IllegalArgumentException("align must be power of two: " + align);
        int aligned = (cursor + align - 1) & ~(align - 1);
        int end = aligned + size;
        if (end > bytes.length) {
            grow(size + align);
            aligned = 0; // fresh buffer starts at 0 with full alignment
            end = size;
        }
        cursor = end;
        return aligned;
    }

    private void grow(int minBytes) {
        int newCap = Math.max(bytes.length * 2, minBytes);
        this.bytes = new byte[newCap];
        this.totalCapacity += newCap;
        this.cursor = 0;
    }

    /** Rewind the bump cursor. Buffer is retained. */
    public void reset() {
        cursor = 0;
    }

    public byte[] bytes() {
        return bytes;
    }

    public int currentCapacity() {
        return bytes.length;
    }

    public int totalCapacity() {
        return totalCapacity;
    }
}
