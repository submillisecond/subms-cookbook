package com.submillisecond.recipes.stats;

/**
 * Log2-spaced CDF buckets. Use to export a full empirical distribution
 * alongside the headline p50/p99 percentiles - downstream tooling can
 * reconstruct any quantile from the cumulative sums without needing
 * the raw sample stream.
 *
 * <p>Byte-equivalent to {@code subms_stats::histogram} in the Rust
 * sibling.
 */
public final class Histogram {
    private Histogram() {}

    /**
     * Log2-spaced histogram of latencies. 64 buckets covering
     * {@code [2^i, 2^(i+1))} nanoseconds for {@code i in 0..64}. A
     * full empirical CDF can be reconstructed from the cumulative
     * sum of this array. Empty input -> all-zero buckets.
     */
    public static long[] cdfBuckets(long[] samples) {
        long[] buckets = new long[64];
        for (long v : samples) {
            int idx;
            if (v == 0L) {
                idx = 0;
            } else {
                // 63 - Long.numberOfLeadingZeros(v) is floor(log2(v)).
                idx = 63 - Long.numberOfLeadingZeros(v);
                if (idx < 0) idx = 0;
                if (idx > 63) idx = 63;
            }
            buckets[idx]++;
        }
        return buckets;
    }
}
