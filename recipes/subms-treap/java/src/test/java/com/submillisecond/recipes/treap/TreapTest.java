package com.submillisecond.recipes.treap;

import org.junit.jupiter.api.Test;

import java.util.HashSet;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
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
}
