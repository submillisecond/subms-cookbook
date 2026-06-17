package com.submillisecond.recipes.arena.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class StatsArenaTest {

    @Test
    void allocationsCounterIncrements() {
        StatsArena a = new StatsArena(256);
        for (int i = 0; i < 5; i++) a.allocate(4, 4);
        assertEquals(5, a.stats().allocations());
    }

    @Test
    void bytesUsedSumsSizes() {
        StatsArena a = new StatsArena(256);
        a.allocate(1, 1);
        a.allocate(4, 4);
        a.allocate(8, 8);
        assertEquals(1 + 4 + 8, a.stats().bytesUsed());
    }

    @Test
    void bytesWastedTracksPadding() {
        StatsArena a = new StatsArena(256);
        // 1-byte alloc leaves cursor at 1; u32 forces 3 bytes padding.
        a.allocate(1, 1);
        a.allocate(4, 4);
        a.allocate(8, 8);
        assertEquals(3, a.stats().bytesWasted());
    }

    @Test
    void peakSurvivesReset() {
        StatsArena a = new StatsArena(1024);
        for (int i = 0; i < 50; i++) a.allocate(8, 8);
        long peakBefore = a.stats().peakBytes();
        assertTrue(peakBefore >= 50 * 8);
        a.reset();
        a.allocate(8, 8);
        assertEquals(peakBefore, a.stats().peakBytes(), "peak persists across reset");
    }

    @Test
    void chunkCountIncrementsOnGrow() {
        StatsArena a = new StatsArena(64);
        assertEquals(1, a.stats().chunkCount());
        for (int i = 0; i < 32; i++) a.allocate(8, 8);
        assertTrue(a.stats().chunkCount() >= 2, "grow must bump chunkCount");
    }

    @Test
    void clearStatsZeroesCounters() {
        StatsArena a = new StatsArena(256);
        for (int i = 0; i < 10; i++) a.allocate(8, 8);
        assertTrue(a.stats().allocations() > 0);
        a.clearStats();
        StatsArena.Stats s = a.stats();
        assertEquals(0, s.allocations());
        assertEquals(0, s.bytesUsed());
        assertEquals(0, s.bytesWasted());
        assertEquals(0, s.peakBytes());
        assertEquals(1, s.chunkCount());
    }

    @Test
    void rejectsNonPowerOfTwoAlignment() {
        StatsArena a = new StatsArena(128);
        org.junit.jupiter.api.Assertions.assertThrows(
            IllegalArgumentException.class,
            () -> a.allocate(4, 3));
    }
}
