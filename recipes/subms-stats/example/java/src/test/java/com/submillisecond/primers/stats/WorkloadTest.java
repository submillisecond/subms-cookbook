package com.submillisecond.primers.stats;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * {@link Workload} contract: deterministic given a seed, shape
 * invariants hold, illegal inputs reject up front.
 */
final class WorkloadTest {

    @Test
    @DisplayName("same seed + shape yields byte-identical sample arrays")
    void deterministic() {
        long[] a = Workload.generate(2_000, 500_000L, Workload.TailShape.CLEAN, 7L);
        long[] b = Workload.generate(2_000, 500_000L, Workload.TailShape.CLEAN, 7L);
        assertArrayEquals(a, b, "Workload must be deterministic under a fixed seed");
    }

    @Test
    @DisplayName("different seeds yield different streams")
    void seedMatters() {
        long[] a = Workload.generate(2_000, 500_000L, Workload.TailShape.CLEAN, 1L);
        long[] b = Workload.generate(2_000, 500_000L, Workload.TailShape.CLEAN, 2L);
        boolean identical = true;
        for (int i = 0; i < a.length; i++) {
            if (a[i] != b[i]) { identical = false; break; }
        }
        assertFalse(identical, "different seeds must produce different sample streams");
    }

    @Test
    @DisplayName("HEAVY tail produces a noticeably larger max than CLEAN at the same base")
    void heavyTailIsHeavier() {
        long[] clean = Workload.generate(20_000, 500_000L, Workload.TailShape.CLEAN, 11L);
        long[] heavy = Workload.generate(20_000, 500_000L, Workload.TailShape.HEAVY, 11L);
        long maxClean = 0, maxHeavy = 0;
        for (long v : clean) if (v > maxClean) maxClean = v;
        for (long v : heavy) if (v > maxHeavy) maxHeavy = v;
        assertTrue(maxHeavy > maxClean * 2,
                "HEAVY max should dwarf CLEAN max; got clean=" + maxClean + " heavy=" + maxHeavy);
    }

    @Test
    @DisplayName("count=0 returns an empty array, no rng work")
    void emptyCount() {
        long[] out = Workload.generate(0, 1_000L, Workload.TailShape.CLEAN, 0L);
        assertEquals(0, out.length, "count=0 must return a zero-length array");
    }

    @Test
    @DisplayName("illegal inputs reject up front")
    void rejectsIllegalInputs() {
        assertThrows(IllegalArgumentException.class,
                () -> Workload.generate(-1, 1_000L, Workload.TailShape.CLEAN, 0L),
                "negative count must reject");
        assertThrows(IllegalArgumentException.class,
                () -> Workload.generate(10, 0L, Workload.TailShape.CLEAN, 0L),
                "zero baseNs must reject");
        assertThrows(IllegalArgumentException.class,
                () -> Workload.generate(10, -5L, Workload.TailShape.CLEAN, 0L),
                "negative baseNs must reject");
    }
}
