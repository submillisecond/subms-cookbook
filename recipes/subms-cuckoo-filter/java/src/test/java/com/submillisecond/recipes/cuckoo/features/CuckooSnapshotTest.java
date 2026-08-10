package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CuckooSnapshotTest {

    @Test
    void snapshotReportsWhatWriterInserted() {
        CuckooFilter cf = new CuckooFilter(1000);
        for (int i = 0; i < 200; i++) cf.insert("k" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);
        for (int i = 0; i < 200; i++) assertTrue(snap.contains("k" + i), "lost k" + i);
        assertEquals(200, snap.size());
    }

    @Test
    void snapshotIsolatedFromWriterMutations() {
        CuckooFilter cf = new CuckooFilter(1000);
        cf.insert("before-snapshot");
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);

        cf.insert("after-snapshot");
        cf.delete("before-snapshot");

        assertTrue(snap.contains("before-snapshot"), "snapshot must remain stable");
        assertFalse(snap.contains("after-snapshot"), "snapshot must not see later inserts");
    }

    @Test
    void emptySnapshotRejectsEverything() {
        CuckooFilter cf = new CuckooFilter(100);
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);
        assertTrue(snap.isEmpty());
        assertFalse(snap.contains("never-inserted"));
    }

    @Test
    void fingerprintCountMatchesSize() {
        CuckooFilter cf = new CuckooFilter(500);
        for (int i = 0; i < 100; i++) cf.insert("k" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);
        assertEquals(snap.size(), snap.fingerprintCount());
    }

    @Test
    void shareableAcrossThreads() throws InterruptedException {
        CuckooFilter cf = new CuckooFilter(1000);
        for (int i = 0; i < 200; i++) cf.insert("k" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);

        List<Thread> threads = new ArrayList<>();
        int[] found = new int[4];
        for (int t = 0; t < 4; t++) {
            final int idx = t;
            threads.add(new Thread(() -> {
                int c = 0;
                for (int i = 0; i < 200; i++) {
                    if (snap.contains("k" + i)) c++;
                }
                found[idx] = c;
            }));
        }
        for (Thread th : threads) th.start();
        for (Thread th : threads) th.join();
        for (int f : found) assertEquals(200, f);
    }

    @Test
    void writerMutationsDoNotResizeSnapshot() {
        CuckooFilter cf = new CuckooFilter(64);
        for (int i = 0; i < 20; i++) cf.insert("k" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);
        int snapBuckets = snap.bucketCount();
        for (int i = 20; i < 40; i++) cf.insert("k" + i);
        assertEquals(snapBuckets, snap.bucketCount());
    }

    @Test
    void snapshotCarriesTheParkedVictim() {
        CuckooFilter cf = new CuckooFilter(1);
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (!cf.insert(key)) break;
            accepted.add(key);
        }
        CuckooSnapshot snap = CuckooSnapshot.capture(cf);
        for (String key : accepted) {
            assertTrue(snap.contains(key), key + " missing from the snapshot");
        }
    }
}
