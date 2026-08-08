package com.submillisecond.recipes.stats;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class RobustTest {

    @Test
    void iqrKnownDistribution() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        assertEquals(50L, Robust.iqr(v));
    }

    @Test
    void madBasic() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i;
        long mad = Robust.medianAbsoluteDeviation(v);
        assertTrue(mad >= 20 && mad <= 30, "MAD around 25: " + mad);
    }

    @Test
    void covConstantSignalIsZero() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = 100L;
        assertTrue(Robust.coefficientOfVariation(v) < 0.001);
    }

    @Test
    void skewnessRightTailPositive() {
        long[] v = new long[1000];
        for (int i = 0; i < 990; i++) v[i] = 100L;
        for (int i = 990; i < 1000; i++) v[i] = 10_000L;
        assertTrue(Robust.skewness(v) > 0.0);
    }

    @Test
    void kurtosisHeavyTailPositive() {
        long[] v = new long[1000];
        for (int i = 0; i < 990; i++) v[i] = 100L;
        for (int i = 990; i < 1000; i++) v[i] = 10_000L;
        assertTrue(Robust.kurtosis(v) > 0.0);
    }

    @Test
    void iqrEmptyZero() {
        assertEquals(0L, Robust.iqr(new long[0]));
    }

    @Test
    void madEmptyZero() {
        assertEquals(0L, Robust.medianAbsoluteDeviation(new long[0]));
    }
}
