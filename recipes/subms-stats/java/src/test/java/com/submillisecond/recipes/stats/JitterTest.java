package com.submillisecond.recipes.stats;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class JitterTest {

    @Test
    void cleanSignalIsLow() {
        long[] samples = new long[100];
        for (int i = 0; i < 100; i++) samples[i] = 100L;
        assertTrue(Jitter.jitterScore(samples) < 0.01);
    }

    @Test
    void returnsZeroForShortInput() {
        assertEquals(0.0, Jitter.jitterScore(new long[0]));
        assertEquals(0.0, Jitter.jitterScore(new long[]{1L, 2L, 3L}));
    }

    @Test
    void noisySignalIsHigher() {
        long[] samples = new long[128];
        for (int w = 0; w < 4; w++) {
            long base = (w % 2 == 0) ? 100L : 1000L;
            for (int i = 0; i < 32; i++) samples[w * 32 + i] = base;
        }
        long[] clean = new long[128];
        for (int i = 0; i < 128; i++) clean[i] = 100L;
        double noisy = Jitter.jitterScore(samples);
        double cleanScore = Jitter.jitterScore(clean);
        assertTrue(noisy > cleanScore, "noisy should exceed clean: " + noisy + " vs " + cleanScore);
        assertTrue(noisy > 0.1, "noisy clears 0.1: " + noisy);
    }

    @Test
    void scoreClampedToUnitInterval() {
        long[] samples = new long[320];
        for (int w = 0; w < 10; w++) {
            long base = (w % 2 == 0) ? 1L : 100_000L;
            for (int i = 0; i < 32; i++) samples[w * 32 + i] = base;
        }
        double s = Jitter.jitterScore(samples);
        assertTrue(s >= 0.0 && s <= 1.0, "score in range: " + s);
    }
}
