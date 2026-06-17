package com.submillisecond.primers.stats;

import com.submillisecond.stats.Bootstrap;

import java.util.Optional;

/**
 * Typed result of {@link Analyser#analyse(String, long[])}. Carries
 * every output from the subms-stats public surface a primer reader
 * cares about. Records the headline percentiles, the tail shape, the
 * robust spread, and a stability indicator for the measurement rig.
 */
public record StatsReport(
        String label,
        int count,
        long p50,
        long p90,
        long p99,
        long p999,
        long max,
        long mean,
        long stddev,
        long cte99,
        double tailFatness,
        Optional<Double> hillIndex,
        long iqr,
        long mad,
        double cv,
        double skewness,
        double kurtosis,
        double jitterScore,
        Bootstrap.CI p99Ci) {
}
