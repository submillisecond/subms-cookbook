package com.submillisecond.recipes.treap.features;

import com.submillisecond.recipes.treap.Treap;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TreapSnapshotTest {

    private static Treap<Integer, Integer> build(int... keys) {
        Treap<Integer, Integer> t = new Treap<>(42);
        for (int k : keys) t.insert(k, k * 10);
        return t;
    }

    @Test
    void emptySnapshotState() {
        Treap<Integer, Integer> t = new Treap<>(0);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        assertTrue(snap.isEmpty());
        assertEquals(0, snap.size());
        assertNull(snap.get(1));
    }

    @Test
    void snapshotGetReturnsTreapValue() {
        Treap<Integer, Integer> t = build(3, 1, 4, 5, 9, 2, 6);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        for (int k : new int[] {1, 2, 3, 4, 5, 6, 9}) {
            assertEquals(k * 10, snap.get(k));
        }
        assertNull(snap.get(999));
    }

    @Test
    void snapshotIsolatedFromSubsequentWrites() {
        Treap<Integer, Integer> t = build(1, 2, 3);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        t.insert(4, 40);
        t.remove(1);
        assertNull(snap.get(4));
        assertEquals(10, snap.get(1));
        assertEquals(3, snap.size());
    }

    @Test
    void iteratorIsSorted() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : snap) keys.add(e.getKey());
        assertEquals(List.of(1, 2, 3, 4, 5, 6, 7, 8, 9), keys);
    }

    @Test
    void rangeYieldsSortedWindow() {
        Treap<Integer, Integer> t = build(1, 2, 3, 4, 5, 6, 7, 8, 9);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        List<Map.Entry<Integer, Integer>> window = snap.range(3, 7);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : window) keys.add(e.getKey());
        assertEquals(List.of(3, 4, 5, 6, 7), keys);
    }

    @Test
    void readersUnderWriterLoad() throws InterruptedException {
        // Take a snapshot, hand it to N reader threads, mutate the source.
        // Every reader must observe the exact snapshot state.
        Treap<Integer, Integer> t = new Treap<>(7);
        for (int i = 0; i < 200; i++) t.insert(i, i * 10);
        final TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);

        AtomicBoolean readerOk = new AtomicBoolean(true);
        Thread[] readers = new Thread[4];
        for (int r = 0; r < readers.length; r++) {
            readers[r] = new Thread(() -> {
                for (int k = 0; k < 200; k++) {
                    Integer v = snap.get(k);
                    if (v == null || v != k * 10) {
                        readerOk.set(false);
                        return;
                    }
                }
            });
            readers[r].start();
        }

        // Concurrent writer churn - snapshot must remain stable.
        for (int k = 200; k < 400; k++) t.insert(k, k * 10);
        for (int k = 0; k < 100; k++) t.remove(k);

        for (Thread th : readers) th.join();
        assertTrue(readerOk.get(), "reader observed mutation");
        assertEquals(200, snap.size());
    }

    @Test
    void entriesViewIsImmutable() {
        Treap<Integer, Integer> t = build(1, 2, 3);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        List<Map.Entry<Integer, Integer>> entries = snap.entries();
        assertEquals(3, entries.size());
        assertThrows(UnsupportedOperationException.class, () ->
                entries.add(new java.util.AbstractMap.SimpleEntry<>(99, 990)));
    }

    @Test
    void rangeOutsideKeysIsEmpty() {
        Treap<Integer, Integer> t = build(10, 20, 30);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        assertTrue(snap.range(100, 200).isEmpty());
        assertTrue(snap.range(-10, -1).isEmpty());
    }

    @Test
    void rangeSingleElement() {
        Treap<Integer, Integer> t = build(5);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        List<Map.Entry<Integer, Integer>> r = snap.range(5, 5);
        assertEquals(1, r.size());
        assertEquals(5, r.get(0).getKey());
        assertEquals(50, r.get(0).getValue());
    }

    @Test
    void emptySnapshotIteratorYieldsNothing() {
        Treap<Integer, Integer> t = new Treap<>(0);
        TreapSnapshot<Integer, Integer> snap = TreapSnapshot.fromTreap(t);
        assertFalse(snap.iterator().hasNext());
    }
}
