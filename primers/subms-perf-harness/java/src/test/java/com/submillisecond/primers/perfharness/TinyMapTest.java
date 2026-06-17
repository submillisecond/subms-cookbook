package com.submillisecond.primers.perfharness;

import java.util.HashSet;
import java.util.NoSuchElementException;
import java.util.Random;
import java.util.Set;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Behavioural tests for the structure-under-test. The harness primer
 * doesn't ship the map as a library, but the bench numbers are only
 * meaningful if the map actually works - so the structure earns its own
 * coverage independent of the bench.
 */
final class TinyMapTest {

    @Test
    @DisplayName("put + get round-trips small batches in insertion order")
    void putGetRoundTrip() {
        TinyMap m = new TinyMap();
        for (int i = 0; i < 100; i++) m.put(i, i * 7);
        for (int i = 0; i < 100; i++) assertEquals(i * 7, m.get(i));
        assertEquals(100, m.size());
    }

    @Test
    @DisplayName("put overwrites without growing size")
    void putOverwrite() {
        TinyMap m = new TinyMap();
        m.put(42L, 1);
        m.put(42L, 2);
        m.put(42L, 3);
        assertEquals(3, m.get(42L));
        assertEquals(1, m.size());
    }

    @Test
    @DisplayName("get on absent key throws")
    void getAbsentThrows() {
        TinyMap m = new TinyMap();
        m.put(1L, 1);
        assertThrows(NoSuchElementException.class, () -> m.get(999L));
    }

    @Test
    @DisplayName("getOrDefault returns sentinel when absent")
    void getOrDefaultAbsent() {
        TinyMap m = new TinyMap();
        m.put(1L, 100);
        assertEquals(100, m.getOrDefault(1L, -1));
        assertEquals(-1,  m.getOrDefault(2L, -1));
    }

    @Test
    @DisplayName("containsKey distinguishes hit from miss")
    void containsKeyHitVsMiss() {
        TinyMap m = new TinyMap();
        m.put(7L, 1);
        assertTrue(m.containsKey(7L));
        assertFalse(m.containsKey(8L));
    }

    @Test
    @DisplayName("remove returns true on hit and the slot is reusable")
    void removeAndReinsert() {
        TinyMap m = new TinyMap();
        m.put(1L, 1);
        m.put(2L, 2);
        assertTrue(m.remove(1L));
        assertFalse(m.containsKey(1L));
        assertEquals(1, m.size());
        m.put(1L, 99);
        assertEquals(99, m.get(1L));
    }

    @Test
    @DisplayName("remove returns false on miss")
    void removeMiss() {
        TinyMap m = new TinyMap();
        m.put(1L, 1);
        assertFalse(m.remove(999L));
        assertTrue(m.containsKey(1L));
    }

    @Test
    @DisplayName("rejects the empty sentinel as a key")
    void rejectsEmptySentinel() {
        TinyMap m = new TinyMap();
        assertThrows(IllegalArgumentException.class,
                () -> m.put(Long.MIN_VALUE, 0));
    }

    @Test
    @DisplayName("grow preserves all entries past load factor")
    void growPreservesEntries() {
        TinyMap m = new TinyMap(16);          // threshold = 12
        int n = 5_000;                         // forces several grows
        for (int i = 0; i < n; i++) m.put(i, i);
        assertEquals(n, m.size());
        for (int i = 0; i < n; i++) assertEquals(i, m.get(i));
        assertTrue(m.capacity() >= n);
    }

    @Test
    @DisplayName("fuzz: matches HashSet semantics under random put/remove")
    void fuzzMatchesHashSet() {
        TinyMap m = new TinyMap();
        Set<Long> ref = new HashSet<>();
        Random r = new Random(7);
        for (int op = 0; op < 20_000; op++) {
            long k = r.nextInt(2_000);   // small domain so removes hit
            if (k == Long.MIN_VALUE) continue;
            if (r.nextBoolean()) {
                m.put(k, (int) k);
                ref.add(k);
            } else {
                boolean removed = m.remove(k);
                boolean expected = ref.remove(k);
                assertEquals(expected, removed,
                        "remove agreement disagreed at op " + op + " key " + k);
            }
        }
        assertEquals(ref.size(), m.size(), "size diverged");
        for (long k : ref) {
            assertTrue(m.containsKey(k), "ref says present, map says absent: " + k);
        }
    }
}
