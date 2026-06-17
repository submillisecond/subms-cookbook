package com.submillisecond.primers.otel;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TinyMapTest {

    @Test
    @DisplayName("put then get on the same key returns the inserted value")
    void putThenGetReturnsValue() {
        TinyMap m = new TinyMap();
        m.put(42L, 100L);
        assertEquals(100L, m.get(42L));
        assertEquals(1, m.size());
    }

    @Test
    @DisplayName("get on an absent key returns the sentinel miss value")
    void getOnMissReturnsSentinel() {
        TinyMap m = new TinyMap();
        m.put(1L, 10L);
        assertEquals(Long.MIN_VALUE, m.get(2L));
    }

    @Test
    @DisplayName("put overwrites the value on a duplicate key without growing size")
    void putOverwritesDuplicateKey() {
        TinyMap m = new TinyMap();
        m.put(7L, 70L);
        m.put(7L, 700L);
        assertEquals(700L, m.get(7L));
        assertEquals(1, m.size());
    }

    @Test
    @DisplayName("the table grows past its initial capacity without losing entries")
    void growthPreservesEntries() {
        TinyMap m = new TinyMap(8);
        int initialCapacity = m.capacity();
        for (long k = 1; k <= 200; k++) m.put(k, k * 3);
        assertTrue(m.capacity() > initialCapacity, "capacity must double past the load-factor threshold");
        for (long k = 1; k <= 200; k++) {
            assertEquals(k * 3, m.get(k), "entry " + k + " survives the grow");
        }
        assertEquals(200, m.size());
    }

    @Test
    @DisplayName("key 0 and zero/negative initial capacity reject up front")
    void rejectsIllegalInputs() {
        TinyMap m = new TinyMap();
        assertThrows(IllegalArgumentException.class, () -> m.put(0L, 1L));
        assertEquals(Long.MIN_VALUE, m.get(0L));
        assertThrows(IllegalArgumentException.class, () -> new TinyMap(0));
        assertThrows(IllegalArgumentException.class, () -> new TinyMap(-4));
    }
}
