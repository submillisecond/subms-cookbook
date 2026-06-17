package com.submillisecond.recipes.lsm.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SnapshotManagerTest {

    @Test
    void snapshotSeesCurrentManifestAtTakeTime() {
        SnapshotManager mgr = new SnapshotManager();
        mgr.publish(new SnapshotManifest(List.of(1L, 2L, 3L)));
        Snapshot s = mgr.snapshot();
        assertEquals(List.of(1L, 2L, 3L), s.sstableIds());
    }

    @Test
    void snapshotIsIsolatedFromSubsequentPublish() {
        SnapshotManager mgr = new SnapshotManager();
        mgr.publish(new SnapshotManifest(List.of(1L)));
        Snapshot s = mgr.snapshot();
        mgr.publish(new SnapshotManifest(List.of(1L, 2L, 3L, 4L)));
        assertEquals(List.of(1L), s.sstableIds(),
                "snapshot must not see post-snapshot publishes");
        assertEquals(List.of(1L, 2L, 3L, 4L), mgr.currentIds());
    }

    @Test
    void multipleConcurrentSnapshotsAreIndependent() {
        SnapshotManager mgr = new SnapshotManager();
        mgr.publish(new SnapshotManifest(List.of(10L)));
        Snapshot s1 = mgr.snapshot();
        mgr.publish(new SnapshotManifest(List.of(10L, 20L)));
        Snapshot s2 = mgr.snapshot();
        mgr.publish(new SnapshotManifest(List.of(10L, 20L, 30L)));
        Snapshot s3 = mgr.snapshot();
        assertEquals(List.of(10L), s1.sstableIds());
        assertEquals(List.of(10L, 20L), s2.sstableIds());
        assertEquals(List.of(10L, 20L, 30L), s3.sstableIds());
    }

    @Test
    void snapshotIdsAreMonotonic() {
        SnapshotManager mgr = new SnapshotManager();
        long a = mgr.snapshot().id();
        long b = mgr.snapshot().id();
        long c = mgr.snapshot().id();
        assertTrue(a < b && b < c);
    }

    @Test
    void snapshotManifestSharedReference() {
        SnapshotManager mgr = new SnapshotManager();
        mgr.publish(new SnapshotManifest(List.of(7L, 8L, 9L)));
        Snapshot s = mgr.snapshot();
        Snapshot also = mgr.snapshot();
        assertSame(s.manifest(), also.manifest(),
                "snapshots taken between publishes share their manifest reference");
    }

    @Test
    void publishWithTransformsCurrent() {
        SnapshotManager mgr = new SnapshotManager();
        mgr.publish(new SnapshotManifest(List.of(1L, 2L)));
        mgr.publishWith(cur -> {
            List<Long> ids = new ArrayList<>(cur.sstableIds());
            ids.add(3L);
            return new SnapshotManifest(ids);
        });
        assertEquals(List.of(1L, 2L, 3L), mgr.currentIds());
    }

    @Test
    void emptyInitialSnapshotIsEmpty() {
        SnapshotManager mgr = new SnapshotManager();
        Snapshot s = mgr.snapshot();
        assertTrue(s.manifest().isEmpty());
        assertEquals(0, s.manifest().size());
    }
}
