package com.submillisecond.recipes.lsm.features;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/**
 * Per-level list of runs. L0 may overlap; L > 0 is key-disjoint after
 * compaction. Runs at L > 0 stay sorted by {@link LeveledRun#minKey()} so
 * overlap searches are linear-scan-friendly.
 */
public final class LeveledManifest {

    public final List<List<LeveledRun>> levels = new ArrayList<>();

    public void push(int level, LeveledRun run) {
        while (levels.size() <= level) levels.add(new ArrayList<>());
        levels.get(level).add(run);
        if (level > 0) {
            levels.get(level).sort(Comparator.comparing(LeveledRun::minKey));
        }
    }

    public int levelRunCount(int level) {
        if (level >= levels.size()) return 0;
        return levels.get(level).size();
    }

    public long levelBytes(int level) {
        if (level >= levels.size()) return 0;
        long t = 0;
        for (LeveledRun r : levels.get(level)) t += r.sizeBytes();
        return t;
    }

    public int totalRunCount() {
        int t = 0;
        for (var l : levels) t += l.size();
        return t;
    }

    /** True if every pair of runs at {@code level} is key-disjoint. L0 is
     *  exempt; this is a level invariant for L > 0. */
    public boolean levelIsNonOverlapping(int level) {
        if (level >= levels.size()) return true;
        List<LeveledRun> runs = levels.get(level);
        for (int i = 0; i < runs.size(); i++) {
            for (int j = i + 1; j < runs.size(); j++) {
                if (runs.get(i).overlaps(runs.get(j))) return false;
            }
        }
        return true;
    }
}
