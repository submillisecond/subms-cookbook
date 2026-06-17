package com.submillisecond.recipes.cms.features;

import com.submillisecond.recipes.cms.CountMinSketch;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Top-K tracker driven by a {@link CountMinSketch}.
 *
 * <p>On every {@link #add(String)} the embedded sketch is updated and the
 * top set is re-checked. The top set is stored as a {@code List<Entry>} of
 * size <= K, kept sorted by estimate descending so reads are O(K). For
 * K <= ~32 the linear scan beats a heap on cache behaviour.
 *
 * <p>Semantics:
 * <ul>
 *   <li>Entries returned by {@link #top()} carry the CMS estimate at the
 *       time of insert/refresh, not a live-recomputed value.</li>
 *   <li>A key already in the top set has its tracked estimate refreshed on
 *       every subsequent {@code add()}.</li>
 *   <li>If a new key ties the K-th existing estimate, the incumbent stays
 *       (no churn).</li>
 * </ul>
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_count_min_sketch::HeavyHitters}.
 */
public final class HeavyHitters {

    public static final class Entry {
        public final String key;
        public final int estimate;

        public Entry(String key, int estimate) {
            this.key = key;
            this.estimate = estimate;
        }
    }

    private final CountMinSketch cms;
    private final int k;
    private final List<Entry> top;

    public HeavyHitters(int k, int depth, int width) {
        this.k = Math.max(1, k);
        this.cms = new CountMinSketch(depth, width);
        this.top = new ArrayList<>(this.k);
    }

    public int k() { return k; }

    public int estimate(String key) {
        return cms.estimate(key);
    }

    /** Increment {@code key} and re-check the top-K. */
    public void add(String key) {
        cms.add(key);
        int est = cms.estimate(key);
        updateTop(key, est);
    }

    /** Current top-K snapshot, sorted by estimate descending. */
    public List<Entry> top() {
        return Collections.unmodifiableList(top);
    }

    private void updateTop(String key, int est) {
        // Already in top? Refresh estimate and re-sort that one entry.
        for (int i = 0; i < top.size(); i++) {
            if (top.get(i).key.equals(key)) {
                top.set(i, new Entry(key, est));
                resortFrom(i);
                return;
            }
        }

        if (top.size() < k) {
            top.add(new Entry(key, est));
            resortFrom(top.size() - 1);
            return;
        }

        // Strict-greater check vs the current floor (last entry).
        Entry floor = top.get(top.size() - 1);
        if (est > floor.estimate) {
            int last = top.size() - 1;
            top.set(last, new Entry(key, est));
            resortFrom(last);
        }
    }

    private void resortFrom(int idx) {
        int i = idx;
        while (i > 0 && top.get(i - 1).estimate < top.get(i).estimate) {
            Collections.swap(top, i - 1, i);
            i--;
        }
        while (i + 1 < top.size() && top.get(i + 1).estimate > top.get(i).estimate) {
            Collections.swap(top, i, i + 1);
            i++;
        }
    }
}
