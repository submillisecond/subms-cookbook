package com.submillisecond.recipes.lsm.features;

import java.util.ArrayList;
import java.util.List;

/**
 * Per-level list of runs. {@code levels.get(0)} is the youngest tier (just
 * flushed); higher indices are older. Empty levels are simply empty lists.
 */
public final class TieredManifest {

    public final List<List<TieredRun>> levels = new ArrayList<>();

    public void push(int level, TieredRun run) {
        while (levels.size() <= level) levels.add(new ArrayList<>());
        levels.get(level).add(run);
    }

    public int levelRunCount(int level) {
        if (level >= levels.size()) return 0;
        return levels.get(level).size();
    }

    public int totalRunCount() {
        int t = 0;
        for (var l : levels) t += l.size();
        return t;
    }
}
