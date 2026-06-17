package com.submillisecond.recipes.lsm.features;

import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;

/**
 * Size-tiered compaction (Cassandra-style): when level L holds at least
 * {@code runsPerLevel} runs of similar size, merge them into one larger run
 * at level L+1. Trades read amplification for write amplification.
 *
 * <p>No file I/O. The base {@link com.submillisecond.recipes.lsm.LsmTree}
 * owns the on-disk format; this class is the pure planning + merging logic
 * a future compaction thread (or test) drives.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_lsm_tree::features::tiered_compaction::TieredCompactionPlanner}.
 */
public final class TieredCompactionPlanner {

    private final int runsPerLevel;

    public TieredCompactionPlanner(int runsPerLevel) {
        this.runsPerLevel = Math.max(2, runsPerLevel);
    }

    public int runsPerLevel() {
        return runsPerLevel;
    }

    /** Lowest level (by index) that has at least {@code runsPerLevel} runs,
     *  or {@code -1} if no level is full. */
    public int pickLevel(TieredManifest manifest) {
        for (int i = 0; i < manifest.levels.size(); i++) {
            if (manifest.levels.get(i).size() >= runsPerLevel) return i;
        }
        return -1;
    }

    /**
     * Merge every run at {@code level} into a single new run, producing a
     * manifest update where {@code level} is empty and {@code level + 1}
     * gains the merged run. Newer-run-wins on key collisions (caller MUST
     * push in order: newest last).
     */
    public void merge(TieredManifest manifest, int level, long newId) {
        List<TieredRun> runs = new ArrayList<>(manifest.levels.get(level));
        manifest.levels.get(level).clear();
        TreeMap<String, byte[]> out = new TreeMap<>();
        // Sentinel marker for tombstone-vs-missing distinction.
        for (TieredRun r : runs) {
            for (TieredRun.Entry e : r.entries) {
                out.put(e.key(), e.value()); // null preserved as tombstone
            }
        }
        List<TieredRun.Entry> merged = new ArrayList<>(out.size());
        for (var e : out.entrySet()) {
            merged.add(new TieredRun.Entry(e.getKey(), e.getValue()));
        }
        manifest.push(level + 1, new TieredRun(newId, merged));
    }
}
