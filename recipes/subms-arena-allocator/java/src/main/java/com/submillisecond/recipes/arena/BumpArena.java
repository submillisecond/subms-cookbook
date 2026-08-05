package com.submillisecond.recipes.arena;

/**
 * Fixed-capacity bump-pointer arena over a {@code byte[]}.
 * {@link #allocate(int, int)} returns an offset into the backing buffer
 * aligned to the requested alignment within the buffer; {@link #reset()}
 * rewinds the bump cursor without freeing the buffer.
 *
 * <p>Base is single-buffer and fixed-capacity. When the buffer is
 * exhausted {@link #allocate(int, int)} throws and
 * {@link #tryAllocate(int, int)} returns {@code -1}. For auto-grow see
 * {@link com.submillisecond.recipes.arena.features.GrowableArena}.
 *
 * <p>Caller writes through {@link #bytes()} at the returned offset.
 *
 * <p><b>Not thread-safe.</b> The cursor is a plain {@code int} with no
 * synchronisation, so two threads calling {@link #allocate(int, int)}
 * concurrently can be handed the same offset and clobber each other. Nothing
 * in this class detects that. Give each thread its own arena; a lock around a
 * shared cursor reintroduces the contended cache line the structure exists to
 * avoid. The same applies to every type in
 * {@code com.submillisecond.recipes.arena.features}.
 */
public final class BumpArena {

    private final byte[] bytes;
    private int cursor;

    public BumpArena(int initialBytes) {
        int cap = Math.max(64, initialBytes);
        this.bytes = new byte[cap];
    }

    /**
     * Return a byte offset aligned to {@code align}, with {@code size}
     * bytes available from it. Throws if the buffer is full.
     */
    public int allocate(int size, int align) {
        int off = tryAllocate(size, align);
        if (off < 0) {
            throw new IllegalStateException(
                "BumpArena out of capacity: cursor=" + cursor
                + " cap=" + bytes.length
                + " size=" + size
                + " align=" + align);
        }
        return off;
    }

    /**
     * Same as {@link #allocate(int, int)} but returns {@code -1} when
     * the request doesn't fit.
     */
    public int tryAllocate(int size, int align) {
        if ((align & (align - 1)) != 0) throw new IllegalArgumentException("align must be power of two: " + align);
        int aligned = (cursor + align - 1) & ~(align - 1);
        int end = aligned + size;
        if (end > bytes.length) {
            return -1;
        }
        cursor = end;
        return aligned;
    }

    /** Rewind the bump cursor. Buffer is retained. */
    public void reset() {
        cursor = 0;
    }

    public byte[] bytes() {
        return bytes;
    }

    public int used() {
        return cursor;
    }

    public int capacity() {
        return bytes.length;
    }

    /** Backwards-compatible alias for {@link #capacity()}. */
    public int currentCapacity() {
        return capacity();
    }

    /** Backwards-compatible alias for {@link #capacity()}. */
    public int totalCapacity() {
        return capacity();
    }
}
