package com.submillisecond.recipes.arena.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class AlignedArenaTest {

    @Test
    void alignmentOnePacksTightly() {
        AlignedArena a = new AlignedArena(256);
        int o1 = a.allocAligned(3, 1);
        int o2 = a.allocAligned(3, 1);
        assertEquals(0, o1);
        assertEquals(3, o2);
        assertEquals(6, a.used());
    }

    @Test
    void alignment64BytePadsCorrectly() {
        AlignedArena a = new AlignedArena(512);
        a.allocAligned(1, 1);
        int off = a.allocAligned(64, 64);
        assertEquals(0, off & 63, "buffer-relative offset is 64-aligned");
        assertTrue(off >= 64, "padding inserted");
    }

    @Test
    void alignment512BytePadsCorrectly() {
        AlignedArena a = new AlignedArena(4096);
        a.allocAligned(7, 1);
        int off = a.allocAligned(128, 512);
        assertEquals(0, off & 511, "buffer-relative offset is 512-aligned");
    }

    @Test
    void rejectsNonPowerOfTwoAlign() {
        AlignedArena a = new AlignedArena(64);
        assertThrows(IllegalArgumentException.class, () -> a.allocAligned(8, 3));
    }

    @Test
    void outOfCapacityReturnsMinusOne() {
        AlignedArena a = new AlignedArena(64);
        a.allocAligned(64, 1);
        assertEquals(-1, a.tryAllocAligned(64, 64));
    }

    @Test
    void outOfCapacityThrows() {
        AlignedArena a = new AlignedArena(64);
        a.allocAligned(64, 1);
        assertThrows(IllegalStateException.class, () -> a.allocAligned(64, 64));
    }

    @Test
    void resetRewindsCursor() {
        AlignedArena a = new AlignedArena(128);
        a.allocAligned(64, 64);
        assertEquals(64, a.used());
        a.reset();
        assertEquals(0, a.used());
        a.allocAligned(64, 64);
        assertEquals(64, a.used());
    }
}
