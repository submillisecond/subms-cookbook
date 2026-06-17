package com.submillisecond.primers.stats;

import com.submillisecond.stats.SubMsSamples;

/**
 * The smallest possible "what does {@link SubMsSamples} give me"
 * snippet. Wrap a {@code long[]}, read the headline numbers off it,
 * print and exit. No comparison, no bootstrap. Read this before
 * {@link StatsMain}.
 */
public final class Demo {
    private Demo() {}

    public static void main(String[] args) {
        long[] raw = Workload.generate(10_000, 800_000L, Workload.TailShape.CLEAN, 42L);

        SubMsSamples s = SubMsSamples.of(raw);

        System.out.printf("n=%d  p50=%dns  p90=%dns  p99=%dns  p99.9=%dns  max=%dns%n",
                s.count(), s.p50(), s.p90(), s.p99(), s.p999(), s.max());
        System.out.printf("mean=%dns  stddev=%dns  jitter=%.3f%n",
                s.mean(), s.stddev(), s.jitterScore());
    }
}
