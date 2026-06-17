package com.submillisecond.recipes.lsm.features;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/**
 * One run inside a {@link LeveledManifest}. Entries are sorted by key on
 * construction so {@link #minKey()} / {@link #maxKey()} are O(1) lookups.
 * Tombstones live as {@code value == null} entries.
 */
public final class LeveledRun {

    public final long id;
    public final List<TieredRun.Entry> entries;

    public LeveledRun(long id, List<TieredRun.Entry> entries) {
        this.id = id;
        List<TieredRun.Entry> sorted = new ArrayList<>(entries);
        sorted.sort(Comparator.comparing(e -> e.key()));
        this.entries = sorted;
    }

    public long sizeBytes() {
        long size = 0;
        for (TieredRun.Entry e : entries) {
            size += e.key().length() + (e.value() == null ? 1 : e.value().length);
        }
        return size;
    }

    public String minKey() {
        return entries.isEmpty() ? null : entries.get(0).key();
    }

    public String maxKey() {
        return entries.isEmpty() ? null : entries.get(entries.size() - 1).key();
    }

    boolean overlaps(LeveledRun other) {
        if (entries.isEmpty() || other.entries.isEmpty()) return false;
        return !(maxKey().compareTo(other.minKey()) < 0
              || other.maxKey().compareTo(minKey()) < 0);
    }
}
