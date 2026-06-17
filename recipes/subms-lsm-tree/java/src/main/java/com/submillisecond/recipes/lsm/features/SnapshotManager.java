package com.submillisecond.recipes.lsm.features;

import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.UnaryOperator;

/**
 * Point-in-time read snapshots over the SSTable manifest.
 *
 * <p>A {@link Snapshot} captures the manifest (list of SSTable IDs) at the
 * moment it is taken. Readers holding a snapshot see the same set of runs
 * even while the writer flushes new memtables or compaction rewrites the
 * manifest underneath them.
 *
 * <p>Mutations swap a new {@link SnapshotManifest} reference in via
 * {@link AtomicReference#set}, leaving outstanding snapshots untouched
 * (each holds its own immutable reference).
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_lsm_tree::features::snapshot::SnapshotManager}.
 */
public final class SnapshotManager {

    private final AtomicReference<SnapshotManifest> current;
    private final AtomicLong nextSnapshotId = new AtomicLong();

    public SnapshotManager() {
        this(new SnapshotManifest(List.of()));
    }

    public SnapshotManager(SnapshotManifest initial) {
        this.current = new AtomicReference<>(initial);
    }

    /** Replace the current manifest. Existing snapshots keep their reference. */
    public void publish(SnapshotManifest manifest) {
        current.set(manifest);
    }

    /** Convenience helper: build a new manifest from the current one. */
    public void publishWith(UnaryOperator<SnapshotManifest> fn) {
        SnapshotManifest cur;
        SnapshotManifest next;
        do {
            cur = current.get();
            next = fn.apply(cur);
        } while (!current.compareAndSet(cur, next));
    }

    /** Take a stable, immutable view of the current manifest. */
    public Snapshot snapshot() {
        SnapshotManifest m = current.get();
        return new Snapshot(nextSnapshotId.getAndIncrement(), m);
    }

    /** Returns the current manifest's sstable IDs. Useful in tests + the read path. */
    public List<Long> currentIds() {
        return current.get().sstableIds();
    }
}
