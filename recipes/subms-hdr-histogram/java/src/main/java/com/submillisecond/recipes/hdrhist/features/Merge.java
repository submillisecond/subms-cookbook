package com.submillisecond.recipes.hdrhist.features;

import com.submillisecond.recipes.hdrhist.HdrHistogram;

/**
 * Sum two histograms with identical shape.
 *
 * <p>Two histograms can be merged if they share significant-digit
 * precision. The merged result is byte-equivalent to recording every
 * value into a single histogram, because each value maps to the same
 * bucket index regardless of which histogram it landed in.
 *
 * <p>Mismatched shapes throw {@link IllegalArgumentException}. Re-record
 * into a fresh histogram at the target precision to merge across
 * precisions.
 */
public final class Merge {

    private Merge() {}

    /** Sum {@code src} into {@code dst}. */
    public static void merge(HdrHistogram dst, HdrHistogram src) {
        dst.addCountsFrom(src);
    }
}
