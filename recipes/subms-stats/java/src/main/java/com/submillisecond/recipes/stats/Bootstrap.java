package com.submillisecond.recipes.stats;

import java.util.Arrays;

/**
 * Bootstrap confidence intervals for percentiles. Reproducible
 * resampling via a deterministic LCG.
 *
 * <p>Use when reporting a percentile to know how WIDE the confidence
 * interval is - "p99 = 230us +- 50us" is meaningful; "p99 = 230us"
 * alone is missing the noise floor.
 *
 * <p>Byte-equivalent to {@code subms_stats::bootstrap} in the Rust
 * sibling.
 */
public final class Bootstrap {
    private Bootstrap() {}

    /**
     * Result of a bootstrap CI: the lower and upper bounds of the
     * confidence interval, in nanoseconds.
     */
    public record CI(long lo, long hi) {}

    /**
     * Bootstrap a percentile estimate with {@code iters} resamples,
     * returning the lower + upper bounds of the {@code confidence}-
     * level interval. {@code confidence} in {@code [0.0, 1.0]};
     * typical value {@code 0.95}.
     *
     * <p>Cost is {@code O(iters * n)}; sensible defaults are
     * {@code iters = 200..1000}, {@code confidence = 0.95}.
     */
    public static CI bootstrapPercentileCi(long[] samples, double q, int iters, double confidence, long seed) {
        if (samples.length == 0 || iters == 0) return new CI(0L, 0L);
        long state = seed | 1L;
        long[] estimates = new long[iters];
        long[] resample = new long[samples.length];
        for (int it = 0; it < iters; it++) {
            for (int i = 0; i < samples.length; i++) {
                state = state * 6364136223846793005L + 1442695040888963407L;
                long idx = state >>> 1;
                resample[i] = samples[(int) (idx % samples.length)];
            }
            long[] sorted = resample.clone();
            Arrays.sort(sorted);
            estimates[it] = Percentiles.percentile(sorted, q);
        }
        Arrays.sort(estimates);
        double loQ = (1.0 - confidence) / 2.0;
        double hiQ = 1.0 - loQ;
        return new CI(
                Percentiles.percentile(estimates, loQ),
                Percentiles.percentile(estimates, hiQ));
    }
}
