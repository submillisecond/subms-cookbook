package com.submillisecond.recipes.tswal;

/**
 * One durably-logged sample. {@code value} round-trips bit-exact through the
 * log via {@link Double#doubleToLongBits}.
 *
 * @param seriesId opaque series identifier
 * @param ts       timestamp (epoch nanos by convention, but the log treats it as
 *                 an opaque i64)
 * @param value    the sample value
 */
public record TsWalRecord(long seriesId, long ts, double value) {
}
