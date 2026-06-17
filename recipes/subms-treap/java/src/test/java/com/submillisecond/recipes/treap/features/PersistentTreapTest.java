package com.submillisecond.recipes.treap.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class PersistentTreapTest {

    @Test
    void emptyPersistentState() {
        PersistentTreap<Integer, Integer> t = new PersistentTreap<>(0);
        assertTrue(t.isEmpty());
        assertEquals(0, t.size());
        assertNull(t.get(1));
    }

    @Test
    void insertReturnsNewVersionWithoutTouchingOld() {
        PersistentTreap<Integer, String> v0 = new PersistentTreap<>(7);
        PersistentTreap<Integer, String> v1 = v0.insert(1, "one");
        assertEquals(0, v0.size());
        assertEquals(1, v1.size());
        assertNull(v0.get(1));
        assertEquals("one", v1.get(1));
    }

    @Test
    void versionChainEachIsolated() {
        PersistentTreap<Integer, Integer> v0 = new PersistentTreap<>(7);
        PersistentTreap<Integer, Integer> v1 = v0.insert(1, 10);
        PersistentTreap<Integer, Integer> v2 = v1.insert(2, 20);
        PersistentTreap<Integer, Integer> v3 = v2.insert(3, 30);

        assertEquals(0, v0.size());
        assertEquals(1, v1.size());
        assertEquals(2, v2.size());
        assertEquals(3, v3.size());

        assertNull(v1.get(2));
        assertEquals(20, v2.get(2));
        assertEquals(30, v3.get(3));
        assertNull(v1.get(3));
    }

    @Test
    void removeLeavesOldVersionIntact() {
        PersistentTreap<Integer, String> v0 = new PersistentTreap<>(7);
        PersistentTreap<Integer, String> v1 = v0.insert(1, "one").insert(2, "two").insert(3, "three");
        PersistentTreap<Integer, String> v2 = v1.remove(2);
        assertEquals("two", v1.get(2), "v1 still has the removed key");
        assertNull(v2.get(2));
        assertEquals(3, v1.size());
        assertEquals(2, v2.size());
    }

    @Test
    void insertReplacesValueInNewVersionOnly() {
        PersistentTreap<Integer, String> v0 = new PersistentTreap<>(7);
        PersistentTreap<Integer, String> v1 = v0.insert(1, "first");
        PersistentTreap<Integer, String> v2 = v1.insert(1, "second");
        assertEquals("first", v1.get(1));
        assertEquals("second", v2.get(1));
        assertEquals(1, v1.size());
        assertEquals(1, v2.size());
    }

    @Test
    void inOrderYieldsSortedKeys() {
        PersistentTreap<Integer, Integer> t = new PersistentTreap<>(123);
        for (int k : new int[] {5, 1, 9, 3, 7, 2, 8}) {
            t = t.insert(k, k * 10);
        }
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Integer> e : t.collectInOrder()) keys.add(e.getKey());
        assertEquals(List.of(1, 2, 3, 5, 7, 8, 9), keys);
    }

    @Test
    void removeAbsentKeyPreservesContents() {
        PersistentTreap<Integer, Integer> v0 = new PersistentTreap<>(0);
        PersistentTreap<Integer, Integer> v1 = v0.insert(1, 1).insert(2, 2);
        PersistentTreap<Integer, Integer> v2 = v1.remove(999);
        assertEquals(v1.size(), v2.size());
        assertEquals(1, v2.get(1));
        assertEquals(2, v2.get(2));
    }

    @Test
    void manyVersionsStress() {
        List<PersistentTreap<Integer, Integer>> versions = new ArrayList<>();
        versions.add(new PersistentTreap<>(99));
        for (int i = 0; i < 200; i++) {
            versions.add(versions.get(versions.size() - 1).insert(i, i * 2));
        }
        for (int i = 0; i < versions.size(); i++) {
            PersistentTreap<Integer, Integer> v = versions.get(i);
            assertEquals(i, v.size());
            for (int k = 0; k < i; k++) {
                assertEquals(k * 2, v.get(k));
            }
        }
    }
}
