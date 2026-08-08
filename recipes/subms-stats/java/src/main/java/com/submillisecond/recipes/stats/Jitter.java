package com.submillisecond.recipes.stats;

/**
 * Measurement-rig stability indicator: did the noise floor shift
 * across the run? Independent of the algorithm's own tail latency.
 *
 * <p>Byte-equivalent to {@code subms_stats::jitter} in the Rust
 * sibling.
 */
public final class Jitter {
    private Jitter() {}

    private static final int WIN = 32;

    /**
     * Jitter score: coefficient of variation of the per-window mean
     * across non-overlapping 32-sample windows. Clamped to
     * {@code [0.0, 1.0]}. Returns 0 when there are fewer than 64
     * samples (two windows).
     *
     * <p>A high jitter score doesn't mean the algorithm is slow - it
     * means the numbers shifted under your feet (GC, NUMA migration,
     * CPU thermal throttling, OS scheduler preemption).
     */
    public static double jitterScore(long[] samples) {
        if (samples.length < WIN * 2) return 0.0;
        int windows = samples.length / WIN;
        double[] means = new double[windows];
        for (int w = 0; w < windows; w++) {
            long sum = 0;
            int start = w * WIN;
            for (int i = 0; i < WIN; i++) sum += samples[start + i];
            means[w] = (double) sum / WIN;
        }
        double grand = 0;
        for (double m : means) grand += m;
        grand /= means.length;
        if (grand <= 0.0) return 0.0;
        double variance = 0;
        for (double m : means) {
            double d = m - grand;
            variance += d * d;
        }
        variance /= means.length;
        double cv = Math.sqrt(variance) / grand;
        return Math.max(0.0, Math.min(1.0, cv));
    }
}
