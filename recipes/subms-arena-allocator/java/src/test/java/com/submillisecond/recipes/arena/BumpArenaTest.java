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
    void growsWhenChunkFull() {
        BumpArena a = new BumpArena(64);
        int firstCap = a.currentCapacity();
        for (int i = 0; i < 32; i++) a.allocate(8, 8);
        assertTrue(a.currentCapacity() >= firstCap, "buffer should have grown");
        assertTrue(a.totalCapacity() > firstCap, "total ever-allocated grew");
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
        int capBefore = a.currentCapacity();
        int totalBefore = a.totalCapacity();
        for (int r = 0; r < 100; r++) {
            for (int i = 0; i < 30; i++) a.allocate(8, 8);
            a.reset();
        }
        assertEquals(capBefore, a.currentCapacity());
        assertEquals(totalBefore, a.totalCapacity());
    }

    @Test
    void minimumCapacityFloor() {
        // Initial capacity of 1 must be promoted to a sensible floor.
        BumpArena a = new BumpArena(1);
        assertTrue(a.currentCapacity() >= 64);
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
    void largeAllocationGrowsAppropriately() {
        BumpArena a = new BumpArena(64);
        a.allocate(256, 8);
        assertTrue(a.currentCapacity() >= 256);
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
