package com.submillisecond.primers.stats;

import com.submillisecond.stats.Bootstrap;

import java.util.Optional;

/**
 * Full primer report. Generates a baseline + a heavy-tailed candidate,
 * prints {@link StatsReport} blocks for each, and runs an A/B compare
 * (KS + Cohen's d) plus a 95% bootstrap CI on the candidate's p99.
 *
 * <p>Run with: {@code java -cp target/classes com.submillisecond.primers.stats.StatsMain}.
 */
public final class StatsMain {

    private static final int SAMPLE_COUNT = 50_000;
    private static final long BASE_NS     = 800_000L;     // ~800us nominal latency
    private static final long BASELINE_SEED  = 1L;
    private static final long CANDIDATE_SEED = 2L;

    private StatsMain() {}

    public static void main(String[] args) {
        long[] baseline = Workload.generate(
                SAMPLE_COUNT, BASE_NS, Workload.TailShape.CLEAN, BASELINE_SEED);
        long[] candidate = Workload.generate(
                SAMPLE_COUNT, BASE_NS, Workload.TailShape.HEAVY, CANDIDATE_SEED);

        StatsReport baseReport = Analyser.analyse("baseline", baseline);
        StatsReport candReport = Analyser.analyse("candidate", candidate);

        printReport(baseReport);
        printReport(candReport);
        printCompare(baseline, candidate);
        printBootstrap(candReport);
    }

    private static void printReport(StatsReport r) {
        System.out.println();
        System.out.println("== " + r.label() + " (n=" + r.count() + ") ==");
        System.out.printf("  percentiles    p50=%dns  p90=%dns  p99=%dns  p99.9=%dns  max=%dns%n",
                r.p50(), r.p90(), r.p99(), r.p999(), r.max());
        System.out.printf("  moments        mean=%dns  stddev=%dns  cv=%.3f  skew=%.3f  kurt=%.3f%n",
                r.mean(), r.stddev(), r.cv(), r.skewness(), r.kurtosis());
        System.out.printf("  robust spread  iqr=%dns  mad=%dns%n",
                r.iqr(), r.mad());
        System.out.printf("  tail           cte99=%dns  fatness(p99/p50)=%.2f  hill(k=50)=%s%n",
                r.cte99(), r.tailFatness(), formatHill(r.hillIndex()));
        System.out.printf("  rig stability  jitterScore=%.3f%n",
                r.jitterScore());
        System.out.printf("  bootstrap      p99 95%% CI = [%dns, %dns]%n",
                r.p99Ci().lo(), r.p99Ci().hi());
    }

    private static void printCompare(long[] baseline, long[] candidate) {
        Optional<Double> ks = Analyser.ks(baseline, candidate);
        Optional<Double> d  = Analyser.cohensD(baseline, candidate);
        System.out.println();
        System.out.println("== compare: candidate vs baseline ==");
        System.out.printf("  KS statistic  = %s   (0.0 identical CDFs, 1.0 disjoint)%n",
                ks.map(v -> String.format("%.4f", v)).orElse("n/a"));
        System.out.printf("  Cohen's d     = %s   (sign: positive = candidate slower)%n",
                d.map(v -> String.format("%+.4f", v)).orElse("n/a"));
    }

    private static void printBootstrap(StatsReport candidate) {
        Bootstrap.CI ci = candidate.p99Ci();
        System.out.println();
        System.out.println("== bootstrap CI (candidate p99, 95%, 500 iters, seed=0) ==");
        System.out.printf("  p99   = %dns%n", candidate.p99());
        System.out.printf("  95%% CI = [%dns, %dns]   width = %dns%n",
                ci.lo(), ci.hi(), Math.max(0L, ci.hi() - ci.lo()));
    }

    private static String formatHill(Optional<Double> hill) {
        return hill.map(v -> String.format("%.3f", v)).orElse("n/a");
    }
}
