package com.submillisecond.recipes.treap.features;

import com.submillisecond.recipes.treap.Treap;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class RangeQueryTest {

    private static Treap<Integer, Integer> build(int... keys) {
        Treap<Integer, Integer> t = new Treap<>(42);
        for (int k : keys) t.insert(k, k * 10);
        return t;
    }

    private static List<Integer> keysOf(RangeQuery<Integer, Integer> q) {
        List<Integer> out = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : q) out.add(e.getKey());
        return out;
    }

    @Test
    void emptyTreapYieldsNothing() {
        Treap<Integer, Integer> t = new Treap<>(0);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, null, true, null, true);
        assertTrue(q.isEmpty());
        assertEquals(0, q.size());
    }

    @Test
    void singleNodeInclusiveMatch() {
        Treap<Integer, Integer> t = build(5);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 5, true, 5, true);
        assertEquals(List.of(5), keysOf(q));
    }

    @Test
    void singleNodeExclusiveMisses() {
        Treap<Integer, Integer> t = build(5);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 5, false, 100, true);
        assertTrue(q.isEmpty());
    }

    @Test
    void inclusiveBoundsYieldSortedWindow() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 3, true, 7, true);
        assertEquals(List.of(3, 4, 5, 6, 7), keysOf(q));
    }

    @Test
    void exclusiveBoundsDropEndpoints() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 3, false, 7, false);
        assertEquals(List.of(4, 5, 6), keysOf(q));
    }

    @Test
    void unboundedBelowIteratesFromMin() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, null, true, 5, true);
        assertEquals(List.of(1, 3, 5), keysOf(q));
    }

    @Test
    void unboundedAboveIteratesToMax() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 5, true, null, true);
        assertEquals(List.of(5, 7, 9), keysOf(q));
    }

    @Test
    void rangeOutsideKeysYieldsNothing() {
        Treap<Integer, Integer> t = build(10, 20, 30);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 100, true, 200, true);
        assertTrue(q.isEmpty());
    }

    @Test
    void valuesMatchKeysInRange() {
        Treap<Integer, Integer> t = build(1, 2, 3, 4, 5);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 2, true, 4, true);
        List<Map.Entry<Integer, Integer>> entries = q.toList();
        assertEquals(3, entries.size());
        for (Map.Entry<Integer, Integer> e : entries) {
            assertEquals(e.getKey() * 10, e.getValue());
        }
    }

    @Test
    void snapshotStableUnderWriterChurn() {
        Treap<Integer, Integer> t = build(1, 2, 3, 4, 5);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 1, true, 5, true);
        // Mutate the source AFTER taking the range snapshot.
        t.insert(6, 60);
        t.remove(3);
        assertEquals(List.of(1, 2, 3, 4, 5), keysOf(q),
                "snapshot must not observe post-snapshot mutations");
    }

    @Test
    void largeTreapInOrderInvariant() {
        Treap<Integer, Integer> t = new Treap<>(99);
        for (int i = 0; i < 1_000; i++) t.insert(i, i);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, 100, true, 899, true);
        List<Integer> keys = keysOf(q);
        assertEquals(800, keys.size());
        for (int i = 1; i < keys.size(); i++) {
            assertTrue(keys.get(i - 1) < keys.get(i));
        }
        assertEquals(100, keys.get(0));
        assertEquals(899, keys.get(keys.size() - 1));
    }

    @Test
    void iteratorIsConsistentWithToList() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7);
        RangeQuery<Integer, Integer> q = RangeQuery.of(t, null, true, null, true);
        assertFalse(q.isEmpty());
        assertEquals(q.toList().size(), keysOf(q).size());
    }
}
