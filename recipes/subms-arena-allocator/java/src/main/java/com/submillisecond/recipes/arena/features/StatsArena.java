package com.submillisecond.recipes.arena.features;

/**
 * Bump arena with live counters for observability. Auto-grows like
 * {@link GrowableArena}; the counters track lifetime aggregates across
 * resets.
 *
 * <p>Tracked:
 * <ul>
 *   <li>{@code allocations} - total {@link #allocate(int, int)} calls
 *       served.</li>
 *   <li>{@code bytesUsed} - sum of allocation sizes (excludes padding).</li>
 *   <li>{@code bytesWasted} - sum of alignment padding inserted between
 *       allocations.</li>
 *   <li>{@code peakBytes} - high-watermark of cursor across this arena's
 *       lifetime.</li>
 *   <li>{@code chunkCount} - number of distinct buffers the arena has
 *       ever opened.</li>
 * </ul>
 *
 * <p>Counters survive {@link #reset()}. {@link #clearStats()} zeros them.
 */
public final class StatsArena {

    /** Immutable snapshot of the live counters. */
    public record Stats(
        long allocations,
        long bytesUsed,
        long bytesWasted,
        long peakBytes,
        long chunkCount) {}

    private byte[] bytes;
    private int cursor;

    private long allocations;
    private long bytesUsed;
    private long bytesWasted;
    private long peakBytes;
    private long chunkCount;

    public StatsArena(int initialBytes) {
        int cap = Math.max(64, initialBytes);
        this.bytes = new byte[cap];
        this.chunkCount = 1;
    }

    /** Allocate {@code size} bytes aligned to {@code align}. Grows on exhaustion. */
    public int allocate(int size, int align) {
        if ((align & (align - 1)) != 0) throw new IllegalArgumentException("align must be power of two: " + align);
        int aligned = (cursor + align - 1) & ~(align - 1);
        int waste = aligned - cursor;
        int end = aligned + size;
        if (end > bytes.length) {
            grow(Math.max(bytes.length * 2, size + align));
            aligned = 0;
            waste = 0;
            end = size;
        }
        cursor = end;
        allocations++;
        bytesUsed += size;
        bytesWasted += waste;
        if (cursor > peakBytes) peakBytes = cursor;
        return aligned;
    }

    private void grow(int newCap) {
        this.bytes = new byte[newCap];
        this.cursor = 0;
        this.chunkCount++;
    }

    /** Rewind the cursor. Counters preserved. */
    public void reset() {
        cursor = 0;
    }

    /** Zero the counters. {@code chunkCount} is reset to 1 (the live buffer). */
    public void clearStats() {
        allocations = 0;
        bytesUsed = 0;
        bytesWasted = 0;
        peakBytes = 0;
        chunkCount = 1;
    }

    public Stats stats() {
        return new Stats(allocations, bytesUsed, bytesWasted, peakBytes, chunkCount);
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
