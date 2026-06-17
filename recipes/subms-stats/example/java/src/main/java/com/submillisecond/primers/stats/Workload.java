package com.submillisecond.primers.stats;

import java.util.SplittableRandom;

/**
 * Synthetic latency generator. Two knobs: a base ns offset (the noise
 * floor) and a {@link TailShape} that controls how often the generator
 * yields a large outlier.
 *
 * <p>Deterministic given a seed. We feed it through {@link Analyser}
 * instead of recorded harness output so the primer is self-contained
 * and reproducible without a CI runner.
 */
public final class Workload {

    /** How heavy the right tail is. */
    public enum TailShape {
        /** Tight distribution; rare, bounded outliers. The "clean" baseline. */
        CLEAN,
        /** Heavy right tail; occasional power-law-ish spikes. The "regressed" candidate. */
        HEAVY,
    }

    private Workload() {}

    /**
     * Generate {@code count} synthetic ns samples around {@code baseNs}.
     * The bulk of the distribution is a tight Gaussian around the base;
     * a small fraction (1% for {@link TailShape#CLEAN}, 5% for
     * {@link TailShape#HEAVY}) is drawn from a heavier branch.
     */
    public static long[] generate(int count, long baseNs, TailShape shape, long seed) {
        if (count < 0) throw new IllegalArgumentException("count < 0");
        if (baseNs <= 0) throw new IllegalArgumentException("baseNs must be positive");
        SplittableRandom rng = new SplittableRandom(seed);
        long[] out = new long[count];
        double tailFraction = (shape == TailShape.HEAVY) ? 0.05 : 0.01;
        // Heavy-tail multiplier: HEAVY yields up to ~20x base, CLEAN up to ~3x.
        double tailScale = (shape == TailShape.HEAVY) ? 20.0 : 3.0;

        for (int i = 0; i < count; i++) {
            double bulk = baseNs + rng.nextGaussian() * (baseNs * 0.05);
            double sample;
            if (rng.nextDouble() < tailFraction) {
                // Inverse-uniform-ish draw - heavier the closer to 0 the uniform is.
                double u = 1.0 - rng.nextDouble();
                sample = bulk + baseNs * (tailScale * (1.0 / u - 1.0));
            } else {
                sample = bulk;
            }
            if (sample < 1.0) sample = 1.0;
            out[i] = (long) sample;
        }
        return out;
    }
}
