package com.submillisecond.recipes.arena;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class BumpArenaTest {

    @Test
    void allocateReturnsAlignedOffsets() {
        BumpArena a = new BumpArena(256);
        int off1 = a.allocate(1, 1);
        int off8 = a.allocate(8, 8);
        assertEquals(0, off1);
        assertEquals(0, off8 & 7, "u64 must be 8-byte aligned");
    }

    @Test
    void resetRewindsCursor() {
        BumpArena a = new BumpArena(128);
        for (int i = 0; i < 8; i++) a.allocate(8, 8);
        a.reset();
        assertEquals(0, a.allocate(8, 8), "reset rewinds to 0");
    }

    @Test
    void fixedCapacityRefusesWhenFull() {
        BumpArena a = new BumpArena(64);
        for (int i = 0; i < 8; i++) a.allocate(8, 8);
        // The 9th 8-byte slot doesn't fit.
        assertEquals(-1, a.tryAllocate(8, 8));
        assertThrows(IllegalStateException.class, () -> a.allocate(8, 8));
    }

    @Test
    void rejectsNonPowerOfTwoAlignment() {
        BumpArena a = new BumpArena(64);
        assertThrows(IllegalArgumentException.class, () -> a.allocate(4, 3));
    }

    @Test
    void writesAreReadBack() {
        BumpArena a = new BumpArena(128);
        int off = a.allocate(4, 4);
        a.bytes()[off]     = 1;
        a.bytes()[off + 1] = 2;
        a.bytes()[off + 2] = 3;
        a.bytes()[off + 3] = 4;
        assertEquals(1, a.bytes()[off]);
        assertEquals(4, a.bytes()[off + 3]);
    }

    @Test
    void manyResetsReuseBuffer() {
        BumpArena a = new BumpArena(256);
        int capBefore = a.capacity();
        for (int r = 0; r < 100; r++) {
            for (int i = 0; i < 30; i++) a.allocate(8, 8);
            a.reset();
        }
        assertEquals(capBefore, a.capacity(), "fixed capacity never changes");
    }

    @Test
    void minimumCapacityFloor() {
        BumpArena a = new BumpArena(1);
        assertTrue(a.capacity() >= 64);
    }

    @Test
    void alignmentOneAlwaysAtCursor() {
        BumpArena a = new BumpArena(64);
        int off1 = a.allocate(1, 1);
        int off2 = a.allocate(1, 1);
        int off3 = a.allocate(1, 1);
        assertEquals(0, off1);
        assertEquals(1, off2);
        assertEquals(2, off3);
    }

    @Test
    void usedTracksCursor() {
        BumpArena a = new BumpArena(128);
        assertEquals(0, a.used());
        a.allocate(8, 8);
        assertEquals(8, a.used());
        a.reset();
        assertEquals(0, a.used());
    }

    @Test
    void resetClearsButPreservesBuffer() {
        BumpArena a = new BumpArena(128);
        a.allocate(64, 1);
        a.reset();
        int off = a.allocate(64, 1);
        assertEquals(0, off);
    }

    @Test
    void allocateZeroByteRegion() {
        BumpArena a = new BumpArena(64);
        int off = a.allocate(0, 1);
        assertTrue(off >= 0);
    }
}
