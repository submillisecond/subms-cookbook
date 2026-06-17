package com.submillisecond.recipes.tsresample;

/**
 * The statistic a resample bucket collapses to. One value emitted per
 * non-empty bucket, at the bucket-start timestamp.
 */
public enum TsResampleMode {
    MEAN,
    LAST,
    FIRST,
    SUM,
    COUNT,
    MIN,
    MAX
}
