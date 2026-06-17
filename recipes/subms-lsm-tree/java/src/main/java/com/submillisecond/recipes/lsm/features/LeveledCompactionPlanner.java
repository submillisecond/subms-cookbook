package com.submillisecond.recipes.lsm.features;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.TreeMap;

/**
 * Leveled compaction (RocksDB style): level L has a soft size budget of
 * {@code base * fanout^(L - 1)} bytes. Each level L &gt; 0 holds runs with
 * disjoint key ranges. Compacting from level L picks one run, finds all
 * overlapping runs at L+1, and merges them into a set of non-overlapping
 * output runs.
 *
 * <p>Tradeoff vs tiered: leveled holds read amp down (one run per key at
 * any level beyond L0) at the cost of higher write amp.
 *
 * <p>No file I/O. Pure manifest + merge planning.
 */
public final class LeveledCompactionPlanner {

    private final long baseBytes;
    private final long fanout;
    private final int l0RunLimit;

    public LeveledCompactionPlanner(long baseBytes, long fanout, int l0RunLimit) {
        this.baseBytes = Math.max(1, baseBytes);
        this.fanout = Math.max(2, fanout);
        this.l0RunLimit = Math.max(1, l0RunLimit);
    }

    public long baseBytes() { return baseBytes; }
    public long fanout() { return fanout; }
    public int l0RunLimit() { return l0RunLimit; }

    public long levelBudget(int level) {
        if (level == 0) return 0;
        long budget = baseBytes;
        for (int i = 1; i < level; i++) {
            long next = budget * fanout;
            // Saturate at Long.MAX_VALUE rather than wrap on overflow.
            if (next / fanout != budget) return Long.MAX_VALUE;
            budget = next;
        }
        return budget;
    }

    /** Lowest level that exceeds its budget (or L0 above its run limit).
     *  Returns the level to compact FROM, or {@code -1} if quiescent. */
    public int pickLevel(LeveledManifest manifest) {
        if (manifest.levelRunCount(0) >= l0RunLimit) return 0;
        for (int l = 1; l < manifest.levels.size(); l++) {
            List<LeveledRun> runs = manifest.levels.get(l);
            if (runs.isEmpty()) continue;
            long bytes = 0;
            for (LeveledRun r : runs) bytes += r.sizeBytes();
            if (bytes > levelBudget(l)) return l;
        }
        return -1;
    }

    /**
     * Compact from {@code fromLevel} into {@code fromLevel + 1}. See the
     * Rust sibling for the algorithm; output replaces every overlapping run
     * at the destination.
     */
    public void compact(LeveledManifest manifest, int fromLevel, long nextId) {
        while (manifest.levels.size() <= fromLevel) {
            manifest.levels.add(new ArrayList<>());
        }
        List<LeveledRun> inputsFrom;
        if (fromLevel == 0) {
            inputsFrom = new ArrayList<>(manifest.levels.get(0));
            manifest.levels.get(0).clear();
        } else {
            List<LeveledRun> l = manifest.levels.get(fromLevel);
            if (l.isEmpty()) return;
            inputsFrom = new ArrayList<>();
            inputsFrom.add(l.remove(0));
        }

        String minKey = null;
        String maxKey = null;
        for (LeveledRun r : inputsFrom) {
            String rmin = r.minKey();
            String rmax = r.maxKey();
            if (rmin != null) {
                minKey = (minKey == null || rmin.compareTo(minKey) < 0) ? rmin : minKey;
            }
            if (rmax != null) {
                maxKey = (maxKey == null || rmax.compareTo(maxKey) > 0) ? rmax : maxKey;
            }
        }

        int destLevel = fromLevel + 1;
        while (manifest.levels.size() <= destLevel) {
            manifest.levels.add(new ArrayList<>());
        }
        List<LeveledRun> overlappingDst = new ArrayList<>();
        if (minKey != null && maxKey != null) {
            List<LeveledRun> dst = manifest.levels.get(destLevel);
            Iterator<LeveledRun> it = dst.iterator();
            while (it.hasNext()) {
                LeveledRun r = it.next();
                String rmin = r.minKey() == null ? "" : r.minKey();
                String rmax = r.maxKey() == null ? "" : r.maxKey();
                boolean overlaps = !(rmax.compareTo(minKey) < 0 || rmin.compareTo(maxKey) > 0);
                if (overlaps) {
                    overlappingDst.add(r);
                    it.remove();
                }
            }
        }

        TreeMap<String, byte[]> out = new TreeMap<>();
        // Destination is older; write first so input shadows it.
        for (LeveledRun r : overlappingDst) {
            for (TieredRun.Entry e : r.entries) {
                out.put(e.key(), e.value());
            }
        }
        for (LeveledRun r : inputsFrom) {
            for (TieredRun.Entry e : r.entries) {
                out.put(e.key(), e.value());
            }
        }
        if (!out.isEmpty()) {
            List<TieredRun.Entry> merged = new ArrayList<>(out.size());
            for (var e : out.entrySet()) merged.add(new TieredRun.Entry(e.getKey(), e.getValue()));
            LeveledRun newRun = new LeveledRun(nextId, merged);
            manifest.push(destLevel, newRun);
        }
    }
}
