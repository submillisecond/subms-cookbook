package com.submillisecond.recipes.treap;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Bounded ordered iteration on the base treap. Range scan is the default
 *  path here, not an opt-in feature. */
final class TreapRangeTest {

    private static Treap<Integer, Integer> build(int... keys) {
        Treap<Integer, Integer> t = new Treap<>(42);
        for (int k : keys) t.insert(k, k * 10);
        return t;
    }

    private static List<Integer> keysOf(Iterable<Map.Entry<Integer, Integer>> q) {
        List<Integer> out = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : q) out.add(e.getKey());
        return out;
    }

    @Test
    void emptyTreapYieldsNothing() {
        Treap<Integer, Integer> t = new Treap<>(0);
        assertEquals(List.of(), keysOf(t.range(null, true, null, true)));
        assertTrue(t.collectRange(null, true, null, true).isEmpty());
    }

    @Test
    void singleNodeInclusiveMatch() {
        assertEquals(List.of(5), keysOf(build(5).range(5, true, 5, true)));
    }

    @Test
    void singleNodeExclusiveMisses() {
        assertEquals(List.of(), keysOf(build(5).range(5, false, 100, true)));
    }

    @Test
    void inclusiveBoundsYieldSortedWindow() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        assertEquals(List.of(3, 4, 5, 6, 7), keysOf(t.range(3, true, 7, true)));
    }

    @Test
    void exclusiveBoundsDropEndpoints() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        assertEquals(List.of(4, 5, 6), keysOf(t.range(3, false, 7, false)));
    }

    @Test
    void mixedBoundsAreIndependent() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7, 2, 8, 4, 6);
        assertEquals(List.of(3, 4, 5, 6), keysOf(t.range(3, true, 7, false)));
        assertEquals(List.of(4, 5, 6, 7), keysOf(t.range(3, false, 7, true)));
    }

    @Test
    void unboundedEndsWalkToTheSpine() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7);
        assertEquals(List.of(1, 3, 5), keysOf(t.range(null, true, 5, true)));
        assertEquals(List.of(5, 7, 9), keysOf(t.range(5, true, null, true)));
        assertEquals(List.of(1, 3, 5, 7, 9), keysOf(t.range(null, true, null, true)));
    }

    @Test
    void rangeOutsideKeysYieldsNothing() {
        Treap<Integer, Integer> t = build(10, 20, 30);
        assertEquals(List.of(), keysOf(t.range(100, true, 200, true)));
        assertEquals(List.of(), keysOf(t.range(-50, true, 0, true)));
    }

    @Test
    void valuesTravelWithTheirKeys() {
        Treap<Integer, Integer> t = build(1, 2, 3, 4, 5);
        List<Map.Entry<Integer, Integer>> entries = t.collectRange(2, true, 4, true);
        assertEquals(3, entries.size());
        for (Map.Entry<Integer, Integer> e : entries) {
            assertEquals(e.getKey() * 10, e.getValue());
        }
    }

    @Test
    void collectRangeSnapshotIsStableUnderChurn() {
        Treap<Integer, Integer> t = build(1, 2, 3, 4, 5);
        List<Map.Entry<Integer, Integer>> window = t.collectRange(1, true, 5, true);
        t.insert(6, 60);
        t.remove(3);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : window) keys.add(e.getKey());
        assertEquals(List.of(1, 2, 3, 4, 5), keys,
                "a materialised window must not observe later mutations");
    }

    @Test
    void largeTreapKeepsTheInOrderInvariant() {
        Treap<Integer, Integer> t = new Treap<>(99);
        for (int i = 0; i < 1_000; i++) t.insert(i, i);
        List<Integer> keys = keysOf(t.range(100, true, 899, true));
        assertEquals(800, keys.size());
        for (int i = 1; i < keys.size(); i++) assertTrue(keys.get(i - 1) < keys.get(i));
        assertEquals(100, keys.get(0));
        assertEquals(899, keys.get(keys.size() - 1));
    }

    @Test
    void lazyRangeAgreesWithTheMaterialisedOne() {
        Treap<Integer, Integer> t = build(5, 1, 9, 3, 7);
        List<Integer> lazy = keysOf(t.range(2, true, 8, true));
        List<Integer> eager = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : t.collectRange(2, true, 8, true)) eager.add(e.getKey());
        assertEquals(eager, lazy);
    }

    @Test
    void exhaustedRangeIteratorThrows() {
        Iterator<Map.Entry<Integer, Integer>> it = build(1).range(1, true, 1, true).iterator();
        assertEquals(1, it.next().getKey());
        assertThrows(NoSuchElementException.class, it::next);
    }
}
