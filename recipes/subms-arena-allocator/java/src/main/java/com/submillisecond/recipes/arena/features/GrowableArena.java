package com.submillisecond.recipes.arena.features;

/**
 * Bump-pointer arena over a {@code byte[]} that auto-grows when the
 * active buffer is exhausted. The new buffer is twice the previous size
 * (or the request size, whichever is larger). On {@link #reset()} only
 * the largest buffer is retained, so steady-state workloads converge on
 * a single buffer after the first round.
 *
 * <p>Trade-off vs the fixed-capacity {@code BumpArena}: grow events are
 * not free - they cost a {@code byte[]} allocation and an L1 cache
 * miss on the first access. The kept buffer is the high-watermark size.
 *
 * <p>Caller writes through {@link #bytes()} at the returned offset.
 * After a grow, {@link #bytes()} points to the new buffer; old offsets
 * are NOT portable across grows. Callers that need stable references
 * across grows should read out before each potentially-growing
 * {@link #allocate(int, int)}.
 */
public final class GrowableArena {

    private byte[] bytes;
    private int cursor;
    private int chunkCount;

    public GrowableArena(int initialBytes) {
        int cap = Math.max(64, initialBytes);
        this.bytes = new byte[cap];
        this.chunkCount = 1;
    }

    /** Allocate {@code size} bytes aligned to {@code align}. Grows on exhaustion. */
    public int allocate(int size, int align) {
        if ((align & (align - 1)) != 0) throw new IllegalArgumentException("align must be power of two: " + align);
        int aligned = (cursor + align - 1) & ~(align - 1);
        int end = aligned + size;
        if (end > bytes.length) {
            grow(Math.max(bytes.length * 2, size + align));
            aligned = 0;
            end = size;
        }
        cursor = end;
        return aligned;
    }

    private void grow(int newCap) {
        this.bytes = new byte[newCap];
        this.chunkCount++;
        this.cursor = 0;
    }

    /** Rewind. The current (largest) buffer is retained. */
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

    /** Number of distinct buffers this arena has opened over its lifetime. */
    public int chunkCount() {
        return chunkCount;
    }
}
