package com.submillisecond.recipes.treap;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TreapTest {

    @Test
    void insertGetRemoveRoundTrip() {
        Treap<Integer, String> t = new Treap<>(7L);
        t.insert(5, "five");
        t.insert(3, "three");
        t.insert(8, "eight");
        t.insert(1, "one");
        assertEquals(4, t.size());
        assertEquals("five", t.get(5));
        assertEquals("three", t.get(3));
        assertNull(t.get(999));

        assertEquals("three", t.remove(3));
        assertEquals(3, t.size());
        assertNull(t.get(3));
        assertNull(t.remove(3));
    }

    @Test
    void insertExistingKeyReplaces() {
        Treap<Integer, String> t = new Treap<>(7L);
        t.insert(1, "first");
        assertEquals("first", t.insert(1, "second"));
        assertEquals(1, t.size());
        assertEquals("second", t.get(1));
    }

    @Test
    void inOrderIsSorted() {
        Treap<Integer, Integer> t = new Treap<>(123L);
        for (int k : new int[]{5, 1, 9, 3, 7, 2, 8}) t.insert(k, k * 10);
        assertEquals(List.of(1, 2, 3, 5, 7, 8, 9), t.collectInOrder());
    }

    @Test
    void supportsThousandRandomKeys() {
        Treap<Integer, Integer> t = new Treap<>(99L);
        java.util.Random r = new java.util.Random(0);
        HashSet<Integer> keys = new HashSet<>();
        for (int i = 0; i < 1000; i++) {
            int k = r.nextInt();
            keys.add(k);
            t.insert(k, k);
        }
        for (Integer k : keys) assertEquals(k, t.get(k));
        assertEquals(keys.size(), t.size());
    }

    @Test
    void emptyState() {
        Treap<Integer, String> t = new Treap<>(0L);
        assertTrue(t.isEmpty());
        assertEquals(0, t.size());
        assertNull(t.get(1));
        assertTrue(t.collectInOrder().isEmpty());
    }

    @Test
    void removeFromEmpty() {
        Treap<Integer, String> t = new Treap<>(0L);
        assertNull(t.remove(1));
    }

    @Test
    void ascendingInserts() {
        Treap<Integer, Integer> t = new Treap<>(5L);
        for (int i = 0; i < 100; i++) t.insert(i, i * 2);
        for (int i = 0; i < 100; i++) assertEquals(i * 2, t.get(i));
        List<Integer> keys = t.collectInOrder();
        for (int i = 1; i < keys.size(); i++) assertTrue(keys.get(i - 1) < keys.get(i));
    }

    @Test
    void descendingInserts() {
        Treap<Integer, Integer> t = new Treap<>(5L);
        for (int i = 99; i >= 0; i--) t.insert(i, i * 2);
        List<Integer> keys = t.collectInOrder();
        for (int i = 1; i < keys.size(); i++) assertTrue(keys.get(i - 1) < keys.get(i));
    }

    @Test
    void removeAllOneByOne() {
        Treap<Integer, String> t = new Treap<>(7L);
        int n = 200;
        for (int i = 0; i < n; i++) t.insert(i, "x");
        for (int i = 0; i < n; i++) {
            assertEquals("x", t.remove(i));
            assertNull(t.get(i));
        }
        assertTrue(t.isEmpty());
    }

    @Test
    void interleavedInsertRemove() {
        Treap<Integer, Integer> t = new Treap<>(11L);
        for (int i = 0; i < 50; i++) t.insert(i, i);
        for (int i = 0; i < 50; i++) if (i % 2 == 0) t.remove(i);
        for (int i = 0; i < 50; i++) {
            if (i % 2 == 0) assertNull(t.get(i));
            else assertEquals(i, t.get(i));
        }
    }

    @Test
    void containsKeyMatchesGet() {
        Treap<Integer, Integer> t = new Treap<>(21L);
        for (int k : new int[]{4, 9, 1, 7}) t.insert(k, k);
        for (int k = 0; k < 12; k++) {
            assertEquals(t.get(k) != null, t.containsKey(k));
        }
    }

    @Test
    void computeAmendsInPlace() {
        Treap<Integer, Long> t = new Treap<>(4L);
        t.insert(10_000, 500L);
        assertEquals(300L, t.compute(10_000, q -> q - 200L));
        assertEquals(300L, t.get(10_000));
        assertNull(t.compute(99, q -> q));
        assertEquals(1, t.size(), "amend does not add a level");
    }

    @Test
    void clearResetsToEmpty() {
        Treap<Integer, Integer> t = new Treap<>(6L);
        for (int i = 0; i < 32; i++) t.insert(i, i);
        t.clear();
        assertTrue(t.isEmpty());
        assertEquals(0, t.size());
        assertEquals(0, t.height());
        assertNull(t.get(5));
        t.insert(5, 5);
        assertEquals(5, t.get(5));
    }

    @Test
    void firstLastAndPopExtremes() {
        Treap<Integer, Long> t = new Treap<>(13L);
        assertNull(t.first());
        assertNull(t.last());
        assertNull(t.popFirst());
        assertNull(t.popLast());

        for (int px : new int[]{9998, 10_001, 9999, 10_000}) t.insert(px, px * 2L);
        assertEquals(9998, t.first().getKey());
        assertEquals(10_001, t.last().getKey());

        assertEquals(19_996L, t.popFirst().getValue());
        assertEquals(20_002L, t.popLast().getValue());
        assertEquals(2, t.size());
        assertEquals(List.of(9999, 10_000), t.collectInOrder());
    }

    @Test
    void popDrainsInKeyOrder() {
        Treap<Integer, Integer> t = new Treap<>(17L);
        for (int i : new int[]{5, 2, 9, 1, 7, 3}) t.insert(i, i);
        List<Integer> drained = new ArrayList<>();
        Map.Entry<Integer, Integer> e;
        while ((e = t.popFirst()) != null) drained.add(e.getKey());
        assertEquals(List.of(1, 2, 3, 5, 7, 9), drained);
        assertTrue(t.isEmpty());
    }

    @Test
    void floorCeilingPredecessorSuccessor() {
        Treap<Integer, Integer> t = new Treap<>(31L);
        for (int k : new int[]{10, 20, 30, 40}) t.insert(k, k);
        assertEquals(20, t.floor(25).getKey());
        assertEquals(30, t.floor(30).getKey());
        assertNull(t.floor(5));

        assertEquals(30, t.ceiling(25).getKey());
        assertEquals(30, t.ceiling(30).getKey());
        assertNull(t.ceiling(41));

        assertEquals(20, t.predecessor(30).getKey());
        assertNull(t.predecessor(10));
        assertEquals(40, t.successor(30).getKey());
        assertNull(t.successor(40));
    }

    @Test
    void navigationAgreesWithASortedScan() {
        Treap<Integer, Integer> t = new Treap<>(77L);
        List<Integer> keys = new ArrayList<>();
        for (int i = 0; i < 200; i++) keys.add(i * 3 + 1);
        List<Integer> shuffled = new ArrayList<>(keys);
        Collections.shuffle(shuffled, new java.util.Random(7));
        for (int k : shuffled) t.insert(k, k);
        assertEquals(keys.size(), t.size());

        for (int probe = 0; probe < 600; probe++) {
            Integer expFloor = null, expCeil = null, expPred = null, expSucc = null;
            for (int k : keys) {
                if (k <= probe) expFloor = k;
                if (k < probe) expPred = k;
                if (expCeil == null && k >= probe) expCeil = k;
                if (expSucc == null && k > probe) expSucc = k;
            }
            assertEquals(expFloor, t.floor(probe) == null ? null : t.floor(probe).getKey());
            assertEquals(expCeil, t.ceiling(probe) == null ? null : t.ceiling(probe).getKey());
            assertEquals(expPred, t.predecessor(probe) == null ? null : t.predecessor(probe).getKey());
            assertEquals(expSucc, t.successor(probe) == null ? null : t.successor(probe).getKey());
        }
    }

    @Test
    void iterationRunsBothDirections() {
        Treap<Integer, Integer> t = new Treap<>(41L);
        for (int k : new int[]{5, 1, 9, 3, 7}) t.insert(k, k * 10);

        List<Integer> up = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : t) up.add(e.getKey());
        assertEquals(List.of(1, 3, 5, 7, 9), up);

        List<Integer> down = new ArrayList<>();
        Iterator<Map.Entry<Integer, Integer>> rev = t.descendingIterator();
        while (rev.hasNext()) down.add(rev.next().getKey());
        assertEquals(List.of(9, 7, 5, 3, 1), down);

        assertEquals("{1: 10, 3: 30, 5: 50, 7: 70, 9: 90}", t.toString());

        Treap<Integer, Integer> empty = new Treap<>(0L);
        assertFalse(empty.iterator().hasNext());
        assertFalse(empty.descendingIterator().hasNext());
        assertThrows(NoSuchElementException.class, () -> empty.iterator().next());
    }

    @Test
    void fromSortedRoundTripsACollectedSnapshot() {
        Treap<Integer, Long> source = new Treap<>(55L);
        for (int i = 0; i < 500; i++) source.insert(i * 7, (long) i);
        List<Map.Entry<Integer, Long>> snapshot = source.collectEntriesInOrder();

        Treap<Integer, Long> rebuilt = Treap.fromSorted(55L, snapshot);
        assertEquals(source.size(), rebuilt.size());
        assertEquals(snapshot, rebuilt.collectEntriesInOrder());
        for (Map.Entry<Integer, Long> e : snapshot) {
            assertEquals(e.getValue(), rebuilt.get(e.getKey()));
        }
        assertTrue(rebuilt.height() < 40, "bulk build stayed balanced, height " + rebuilt.height());
    }

    @Test
    void fromSortedRejectsUnsortedAndDuplicateInput() {
        List<Map.Entry<Integer, String>> outOfOrder = List.of(
                Map.entry(1, "a"), Map.entry(3, "b"), Map.entry(2, "c"));
        IllegalArgumentException e1 = assertThrows(
                IllegalArgumentException.class, () -> Treap.fromSorted(1L, outOfOrder));
        assertTrue(e1.getMessage().contains("index 2"), e1.getMessage());

        List<Map.Entry<Integer, String>> duplicate = List.of(Map.entry(1, "a"), Map.entry(1, "b"));
        assertThrows(IllegalArgumentException.class, () -> Treap.fromSorted(1L, duplicate));

        Treap<Integer, String> empty = Treap.fromSorted(1L, List.of());
        assertTrue(empty.isEmpty());
        assertEquals(0, empty.height());
    }

    @Test
    void heightTracksTheRandomizedBound() {
        // Deterministic: fixed seed, fixed key stream. The randomized-priority
        // bound is ~3*ln(n) expected; the assertion is loose enough to be a
        // regression guard on the priority stream, not a test of the constant.
        int n = 20_000;
        Treap<Integer, Integer> t = new Treap<>(2024L);
        int x = 0xDEECE66D;
        for (int i = 0; i < n; i++) {
            x = x * 1664525 + 1013904223;
            t.insert(x, x);
        }
        double expected = 3.0 * Math.log(t.size());
        assertTrue(t.height() < 2.0 * expected,
                "height " + t.height() + " against 3*ln(n) = " + expected + " - priority stream degraded?");
        assertTrue(t.height() >= Math.log(t.size()) / Math.log(2) - 1.0, "height below the floor");
    }

    @Test
    void withRandomSeedStillBuildsABalancedTree() {
        // Output is not reproducible by construction, so the assertion is on
        // the shape, not on a value.
        Treap<Integer, Integer> t = Treap.withRandomSeed();
        for (int i = 0; i < 4_096; i++) t.insert(i, i);
        assertEquals(4_096, t.size());
        assertTrue(t.height() < 6.0 * Math.log(4_096));
        assertEquals(0, t.first().getKey());
    }

    @Test
    void splitOffCutsAtThePivot() {
        Treap<Integer, Long> book = new Treap<>(19L);
        for (int px = 9_990; px < 10_010; px++) book.insert(px, (long) px);
        Treap<Integer, Long> marketable = book.splitOff(10_000);
        assertEquals(10, book.size());
        assertEquals(10, marketable.size());
        assertEquals(9_999, book.last().getKey());
        assertEquals(10_000, marketable.first().getKey());
        for (Map.Entry<Integer, Long> e : book) assertTrue(e.getKey() < 10_000);
        for (Map.Entry<Integer, Long> e : marketable) assertTrue(e.getKey() >= 10_000);
        assertEquals(10_005L, marketable.get(10_005));
        assertTrue(marketable.height() >= 1);
    }

    @Test
    void splitOffHandlesTheDegeneratePivots() {
        Treap<Integer, Integer> t = new Treap<>(23L);
        for (int k = 0; k < 8; k++) t.insert(k, k);
        Treap<Integer, Integer> all = t.splitOff(-1);
        assertTrue(t.isEmpty(), "pivot below every key takes the whole tree");
        assertEquals(8, all.size());

        Treap<Integer, Integer> t2 = new Treap<>(23L);
        for (int k = 0; k < 8; k++) t2.insert(k, k);
        assertTrue(t2.splitOff(100).isEmpty());
        assertEquals(8, t2.size());

        assertTrue(new Treap<Integer, Integer>(1L).splitOff(0).isEmpty());
    }

    @Test
    void splitThenJoinRoundTrips() {
        Treap<Integer, Long> book = new Treap<>(29L);
        for (int px = 9_990; px < 10_010; px++) book.insert(px, px * 2L);
        List<Map.Entry<Integer, Long>> before = book.collectEntriesInOrder();

        book.join(book.splitOff(10_000));
        assertEquals(20, book.size());
        assertEquals(before, book.collectEntriesInOrder());
        assertTrue(book.height() < 20, "rejoined tree stayed shallow");
    }

    @Test
    void joinRefusesOverlappingRanges() {
        Treap<Integer, Integer> lo = new Treap<>(31L);
        for (int k = 0; k < 5; k++) lo.insert(k, k);
        Treap<Integer, Integer> overlapping = new Treap<>(37L);
        for (int k = 3; k < 8; k++) overlapping.insert(k, k);
        assertThrows(IllegalArgumentException.class, () -> lo.join(overlapping));
        assertEquals(5, lo.size(), "refused join left the receiver untouched");
        assertEquals(5, overlapping.size(), "and the donor too");

        lo.join(new Treap<>(41L));
        assertEquals(5, lo.size());

        Treap<Integer, Integer> fresh = new Treap<>(43L);
        Treap<Integer, Integer> donor = new Treap<>(47L);
        for (int k = 0; k < 4; k++) donor.insert(k, k);
        fresh.join(donor);
        assertEquals(List.of(0, 1, 2, 3), fresh.collectInOrder());
        assertTrue(donor.isEmpty(), "a joined donor is drained");
    }

    @Test
    void ascendingKeysDoNotBuildASpine() {
        // The failure mode the SplitMix64 finalizer exists to prevent: a key
        // stream correlated with the priority stream degenerating to O(n) depth.
        int n = 20_000;
        Treap<Integer, Integer> t = new Treap<>(1L);
        for (int i = 0; i < n; i++) t.insert(i, i);
        assertTrue(t.height() < 2.0 * 3.0 * Math.log(n),
                "ascending inserts stayed logarithmic, height " + t.height());
    }
}
