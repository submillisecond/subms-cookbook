package com.submillisecond.recipes.treap.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SplittableTreapTest {

    private static SplittableTreap<Integer, Integer> build(long seed, int... keys) {
        SplittableTreap<Integer, Integer> t = new SplittableTreap<>(seed);
        for (int k : keys) t.insert(k, k * 10);
        return t;
    }

    private static List<Integer> keysOf(SplittableTreap<Integer, Integer> t) {
        List<Integer> out = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : t.collectInOrder()) out.add(e.getKey());
        return out;
    }

    @Test
    void emptySplitYieldsTwoEmpties() {
        SplittableTreap<Integer, Integer> t = new SplittableTreap<>(0);
        SplittableTreap.Split<Integer, Integer> s = t.split(5);
        assertTrue(s.left.isEmpty());
        assertTrue(s.right.isEmpty());
    }

    @Test
    void singleNodeSplitBelowPivot() {
        SplittableTreap<Integer, Integer> t = build(0, 5);
        SplittableTreap.Split<Integer, Integer> s = t.split(10);
        assertEquals(1, s.left.size());
        assertTrue(s.right.isEmpty());
        assertEquals(50, s.left.get(5));
    }

    @Test
    void singleNodeSplitAbovePivot() {
        SplittableTreap<Integer, Integer> t = build(0, 5);
        SplittableTreap.Split<Integer, Integer> s = t.split(1);
        assertTrue(s.left.isEmpty());
        assertEquals(1, s.right.size());
        assertEquals(50, s.right.get(5));
    }

    @Test
    void splitAtExistingKeyPutsKeyOnRight() {
        SplittableTreap<Integer, Integer> t = build(7, 1, 2, 3, 4, 5);
        SplittableTreap.Split<Integer, Integer> s = t.split(3);
        assertEquals(List.of(1, 2), keysOf(s.left));
        assertEquals(List.of(3, 4, 5), keysOf(s.right));
    }

    @Test
    void splitThenMergeRoundTrips() {
        SplittableTreap<Integer, Integer> t = build(7, 5, 1, 9, 3, 7, 2, 8, 4, 6);
        List<Integer> original = keysOf(t);
        // collectInOrder above didn't drain t; re-collect now to capture pre-split state.
        SplittableTreap<Integer, Integer> t2 = build(7, 5, 1, 9, 3, 7, 2, 8, 4, 6);
        SplittableTreap.Split<Integer, Integer> s = t2.split(5);
        SplittableTreap<Integer, Integer> merged = SplittableTreap.merge(s.left, s.right);
        assertEquals(original, keysOf(merged));
        assertEquals(9, merged.size());
    }

    @Test
    void mergeDisjointTreapsPreservesOrder() {
        SplittableTreap<Integer, Integer> lo = build(7, 1, 2, 3);
        SplittableTreap<Integer, Integer> hi = build(11, 4, 5, 6);
        SplittableTreap<Integer, Integer> merged = SplittableTreap.merge(lo, hi);
        assertEquals(List.of(1, 2, 3, 4, 5, 6), keysOf(merged));
        assertEquals(6, merged.size());
    }

    @Test
    void mergeWithEmptyLeftReturnsRight() {
        SplittableTreap<Integer, Integer> lo = new SplittableTreap<>(0);
        SplittableTreap<Integer, Integer> hi = build(7, 1, 2, 3);
        SplittableTreap<Integer, Integer> merged = SplittableTreap.merge(lo, hi);
        assertEquals(3, merged.size());
        assertEquals(20, merged.get(2));
    }

    @Test
    void mergeWithEmptyRightReturnsLeft() {
        SplittableTreap<Integer, Integer> lo = build(7, 1, 2, 3);
        SplittableTreap<Integer, Integer> hi = new SplittableTreap<>(0);
        SplittableTreap<Integer, Integer> merged = SplittableTreap.merge(lo, hi);
        assertEquals(3, merged.size());
        assertEquals(20, merged.get(2));
    }

    @Test
    void splitAtPivotBelowAllKeys() {
        SplittableTreap<Integer, Integer> t = build(7, 10, 20, 30);
        SplittableTreap.Split<Integer, Integer> s = t.split(5);
        assertTrue(s.left.isEmpty());
        assertEquals(3, s.right.size());
    }

    @Test
    void splitAtPivotAboveAllKeys() {
        SplittableTreap<Integer, Integer> t = build(7, 10, 20, 30);
        SplittableTreap.Split<Integer, Integer> s = t.split(100);
        assertEquals(3, s.left.size());
        assertTrue(s.right.isEmpty());
    }

    @Test
    void mergeRejectsOverlappingKeys() {
        SplittableTreap<Integer, Integer> lo = build(7, 1, 5);
        SplittableTreap<Integer, Integer> hi = build(11, 3, 8);
        assertThrows(IllegalArgumentException.class, () -> SplittableTreap.merge(lo, hi));
    }

    @Test
    void largeSplitMergeRoundTrip() {
        SplittableTreap<Integer, Integer> t = new SplittableTreap<>(42);
        for (int i = 0; i < 500; i++) t.insert(i, i);
        List<Integer> original = keysOf(t);
        // After collectInOrder t still holds its tree; split will drain it.
        SplittableTreap.Split<Integer, Integer> s = t.split(250);
        assertEquals(250, s.left.size());
        assertEquals(250, s.right.size());
        SplittableTreap<Integer, Integer> merged = SplittableTreap.merge(s.left, s.right);
        assertEquals(original, keysOf(merged));
    }

    @Test
    void getOnEmptyTreapIsNull() {
        SplittableTreap<Integer, Integer> t = new SplittableTreap<>(0);
        assertNull(t.get(99));
    }
}
