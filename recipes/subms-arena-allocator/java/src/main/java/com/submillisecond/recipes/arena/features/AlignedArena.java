package com.submillisecond.recipes.arena.features;

/**
 * Bump arena exposing an explicit {@code allocAligned(size, align)}
 * convenience.
 *
 * <p>Important caveat: the JVM does not expose portable control over
 * the absolute address of a {@code byte[]}. {@link #allocAligned} maps
 * the requested alignment onto the buffer-relative offset; the absolute
 * address {@code (bytes() + offset)} only inherits the JVM's byte[]
 * base alignment (typically 8 or 16 on HotSpot). SIMD code reading via
 * an aligned vector load through {@code Unsafe}/{@code MemorySegment}
 * must use {@code Foreign Memory API} buffers, not heap byte[]. This
 * class is the API mirror of the Rust feature; it preserves the
 * intra-buffer alignment math without claiming absolute-address
 * alignment.
 *
 * <p>Fixed-capacity, single buffer. Throws when out of room; use
 * {@link #tryAllocAligned(int, int)} for the fallible form.
 */
public final class AlignedArena {

    private final byte[] bytes;
    private int cursor;

    public AlignedArena(int capacity) {
        int cap = Math.max(64, capacity);
        this.bytes = new byte[cap];
    }

    /**
     * Allocate {@code size} bytes at offset aligned-to-{@code align}
     * within the backing buffer. Throws when out of capacity.
     */
    public int allocAligned(int size, int align) {
        int off = tryAllocAligned(size, align);
        if (off < 0) {
            throw new IllegalStateException(
                "AlignedArena out of capacity: cursor=" + cursor
                + " cap=" + bytes.length + " size=" + size + " align=" + align);
        }
        return off;
    }

    /** Fallible variant; returns {@code -1} when out of capacity. */
    public int tryAllocAligned(int size, int align) {
        if ((align & (align - 1)) != 0) throw new IllegalArgumentException("align must be power of two: " + align);
        int aligned = (cursor + align - 1) & ~(align - 1);
        int end = aligned + size;
        if (end > bytes.length) {
            return -1;
        }
        cursor = end;
        return aligned;
    }

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
}
