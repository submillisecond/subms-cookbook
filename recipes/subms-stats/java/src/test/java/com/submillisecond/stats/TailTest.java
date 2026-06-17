package com.submillisecond.stats;

import org.junit.jupiter.api.Test;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TailTest {

    @Test
    void cteExceedsQuantile() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        long cte = Tail.conditionalTailExpectation(v, 0.95);
        assertTrue(cte >= 95, "CTE >= cutoff: " + cte);
    }

    @Test
    void cteEmptyIsZero() {
        assertEquals(0L, Tail.conditionalTailExpectation(new long[0], 0.99));
    }

    @Test
    void hillReturnsEmptyForTinyInput() {
        assertFalse(Tail.hillTailIndex(new long[]{1L, 2L, 3L}, 5).isPresent());
    }

    @Test
    void hillPowerlikeTailReturnsPositive() {
        long[] v = new long[999];
        for (int i = 0; i < 999; i++) v[i] = ((long) (i + 1)) * (i + 1);
        Optional<Double> idx = Tail.hillTailIndex(v, 50);
        assertTrue(idx.isPresent());
        assertTrue(idx.get() > 0.0, "Hill on power-law: " + idx.get());
    }

    @Test
    void fatnessRatioUniformCloseToOne() {
        long[] v = new long[1000];
        for (int i = 0; i < 1000; i++) v[i] = 100L;
        double r = Tail.tailFatnessRatio(v);
        assertTrue(Math.abs(r - 1.0) < 0.01);
    }

    @Test
    void fatnessRatioHeavyTailExceedsOne() {
        long[] v = new long[1000];
        for (int i = 0; i < 990; i++) v[i] = 100L;
        for (int i = 990; i < 1000; i++) v[i] = 10_000L;
        double r = Tail.tailFatnessRatio(v);
        assertTrue(r > 1.0);
    }

    @Test
    void fatnessRatioEmptyZero() {
        assertEquals(0.0, Tail.tailFatnessRatio(new long[0]));
    }
}
