package com.submillisecond.recipes.cms.features;

import com.submillisecond.recipes.cms.CountMinSketch;

/**
 * Sliding-window CMS as a ring of N sub-sketches.
 *
 * <p>Each sub-sketch covers one time slice (the caller defines the time
 * unit by when they call {@link #tick()}). {@code add()} writes only to
 * the current slice. {@code estimate()} sums per-slice estimates, which
 * upper-bounds the true count over the window. {@code tick()} advances
 * the ring and clears the now-current slice.
 *
 * <p>The summed bound is always >= the true count over the window but is
 * not as tight as a single CMS of the same total width - conservative-update
 * is non-additive across sub-sketches.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_count_min_sketch::WindowedCountMinSketch}.
 */
public final class WindowedCountMinSketch {

    private final CountMinSketch[] sketches;
    private int head;

    public WindowedCountMinSketch(int slices, int depth, int width) {
        int n = Math.max(2, slices);
        this.sketches = new CountMinSketch[n];
        for (int i = 0; i < n; i++) {
            this.sketches[i] = new CountMinSketch(depth, width);
        }
        this.head = 0;
    }

    public int slices() { return sketches.length; }
    public int depth()  { return sketches[0].depth(); }
    public int width()  { return sketches[0].width(); }

    public void add(String key) {
        sketches[head].add(key);
    }

    /** Window-wide estimate: sum across all slices, saturating at INT_MAX. */
    public int estimate(String key) {
        long total = 0;
        for (CountMinSketch s : sketches) {
            total += s.estimate(key);
            if (total > Integer.MAX_VALUE) return Integer.MAX_VALUE;
        }
        return (int) total;
    }

    /** Estimate restricted to the current (head) slice. */
    public int estimateCurrent(String key) {
        return sketches[head].estimate(key);
    }

    /**
     * Advance the ring: the slice immediately behind {@code head} becomes
     * the new head and is cleared. The previously-oldest slice is the one
     * that gets overwritten.
     */
    public void tick() {
        head = (head + 1) % sketches.length;
        sketches[head].clearAll();
    }
}
