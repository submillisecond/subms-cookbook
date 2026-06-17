package com.submillisecond.recipes.lsm.features;

import java.util.List;

/**
 * Handle to one read view. Cheap to share - just wraps a reference to an
 * immutable {@link SnapshotManifest}.
 */
public final class Snapshot {

    private final long id;
    private final SnapshotManifest manifest;

    Snapshot(long id, SnapshotManifest manifest) {
        this.id = id;
        this.manifest = manifest;
    }

    public long id() {
        return id;
    }

    public SnapshotManifest manifest() {
        return manifest;
    }

    public List<Long> sstableIds() {
        return manifest.sstableIds();
    }
}
