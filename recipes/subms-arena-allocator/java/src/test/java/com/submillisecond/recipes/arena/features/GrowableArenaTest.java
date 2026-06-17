package com.submillisecond.recipes.arena.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class GrowableArenaTest {

    @Test
    void allocatesAndReadsBack() {
        GrowableArena a = new GrowableArena(256);
        int off = a.allocate(4, 4);
        a.bytes()[off] = 0x7f;
        assertEquals(0x7f, a.bytes()[off]);
    }

    @Test
    void growsAtBoundary() {
        GrowableArena a = new GrowableArena(64);
        int firstCap = a.capacity();
        for (int i = 0; i < 32; i++) a.allocate(8, 8);
        assertTrue(a.capacity() > firstCap, "should have grown");
        assertTrue(a.chunkCount() >= 2);
    }

    @Test
    void largeAllocationTriggersGrow() {
        GrowableArena a = new GrowableArena(64);
        int off = a.allocate(256, 8);
        assertTrue(a.capacity() >= 256);
        assertEquals(0, off, "first alloc in fresh chunk is at 0");
    }

    @Test
    void alignmentRespectedAcrossGrow() {
        GrowableArena a = new GrowableArena(64);
        // Force a grow via lots of u64 allocations, then verify
        // alignment is still honoured.
        for (int i = 0; i < 16; i++) a.allocate(8, 8);
        int off = a.allocate(8, 8);
        assertEquals(0, off & 7, "8-byte alignment after grow");
    }

    @Test
    void resetSteadyState() {
        GrowableArena a = new GrowableArena(64);
        // First round grows, subsequent rounds within the same kept
        // buffer must not grow again.
        for (int i = 0; i < 32; i++) a.allocate(8, 8);
        int afterFirst = a.capacity();
        a.reset();
        for (int i = 0; i < 32; i++) a.allocate(8, 8);
        a.reset();
        assertEquals(afterFirst, a.capacity());
    }

    @Test
    void rejectsNonPowerOfTwoAlignment() {
        GrowableArena a = new GrowableArena(128);
        org.junit.jupiter.api.Assertions.assertThrows(
            IllegalArgumentException.class,
            () -> a.allocate(4, 3));
    }
}
