package com.submillisecond.recipes.art.features;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.art.Art;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ArtSnapshotTest {

    @Test
    void emptySnapshot() {
        Art<Integer> t = new Art<>();
        ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);
        assertTrue(snap.isEmpty());
        assertEquals(0, snap.size());
        assertNull(snap.get("anything".getBytes()));
    }

    @Test
    void snapshotMatchesTreeAtFreezeTime() {
        Art<Integer> t = new Art<>();
        t.insert("alpha".getBytes(), 1);
        t.insert("beta".getBytes(), 2);
        ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);
        assertEquals(2, snap.size());
        assertEquals(1, snap.get("alpha".getBytes()));
        assertEquals(2, snap.get("beta".getBytes()));
    }

    @Test
    void snapshotIsolatedFromWriterMutations() {
        Art<Integer> t = new Art<>();
        t.insert("alpha".getBytes(), 1);
        t.insert("beta".getBytes(), 2);
        ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);

        // Writer mutates after snapshot.
        t.insert("gamma".getBytes(), 3);
        t.insert("alpha".getBytes(), 99);

        assertEquals(1, snap.get("alpha".getBytes()), "snapshot frozen");
        assertEquals(2, snap.get("beta".getBytes()));
        assertNull(snap.get("gamma".getBytes()), "post-snapshot insert invisible");
        assertEquals(2, snap.size());
    }

    @Test
    void snapshotIterationIsInByteOrder() {
        Art<Integer> t = new Art<>();
        String[] in = {"banana", "apple", "cherry", "avocado"};
        for (int i = 0; i < in.length; i++) t.insert(in[i].getBytes(), i);
        ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);
        List<String> keys = new ArrayList<>();
        for (Map.Entry<byte[], Integer> e : snap.entries()) {
            keys.add(new String(e.getKey()));
        }
        assertEquals(List.of("apple", "avocado", "banana", "cherry"), keys);
    }

    @Test
    void snapshotSharableAcrossThreads() throws Exception {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 100; i++) {
            String k = String.format("k%03d", i);
            t.insert(k.getBytes(), i);
        }
        final ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);
        ExecutorService es = Executors.newFixedThreadPool(4);
        try {
            List<Future<Integer>> futures = new ArrayList<>();
            for (int j = 0; j < 4; j++) {
                futures.add(es.submit(() -> {
                    int hits = 0;
                    for (int i = 0; i < 100; i++) {
                        String k = String.format("k%03d", i);
                        Integer got = snap.get(k.getBytes());
                        if (got != null && got == i) hits++;
                    }
                    return hits;
                }));
            }
            for (Future<Integer> f : futures) {
                assertEquals(100, f.get());
            }
        } finally {
            es.shutdownNow();
        }
    }

    @Test
    void snapshotEntriesAreImmutable() {
        Art<Integer> t = new Art<>();
        t.insert("a".getBytes(), 1);
        ArtSnapshot<Integer> snap = ArtSnapshot.fromTree(t);
        List<Map.Entry<byte[], Integer>> es = snap.entries();
        assertThrows(UnsupportedOperationException.class, () -> es.add(Map.entry("zzz".getBytes(), 99)));
    }
}
