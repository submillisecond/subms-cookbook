package com.submillisecond.recipes.lsm.features;

import java.util.List;

/**
 * Immutable per-snapshot view: an ordered list of SSTable ids that were
 * live when the snapshot was opened. Newest-last; the LSM read path walks
 * this in reverse.
 */
public final class SnapshotManifest {

    private final List<Long> sstableIds;

    public SnapshotManifest(List<Long> sstableIds) {
        // Defensive copy + unmodifiable so a snapshot holder can never mutate.
        this.sstableIds = List.copyOf(sstableIds);
    }

    public List<Long> sstableIds() {
        return sstableIds;
    }

    public int size() {
        return sstableIds.size();
    }

    public boolean isEmpty() {
        return sstableIds.isEmpty();
    }
}
