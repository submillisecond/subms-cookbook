package com.submillisecond.recipes.art.features;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.art.Art;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CompactionTest {

    @Test
    void deleteReturnsPriorValue() {
        Art<Integer> t = new Art<>();
        t.insert("alpha".getBytes(), 1);
        t.insert("beta".getBytes(), 2);
        assertEquals(1, Compaction.delete(t, "alpha".getBytes()));
        assertNull(Compaction.delete(t, "alpha".getBytes()), "second delete is a no-op");
        assertEquals(1, t.size());
        assertEquals(2, t.get("beta".getBytes()));
    }

    @Test
    void compactShrinksFullBackToSmall() {
        Art<Integer> t = new Art<>();
        // Grow root to Full (5+ distinct first bytes).
        for (int i = 0; i < 10; i++) {
            t.insert(new byte[]{(byte) i}, i);
        }
        // Delete down to 3 occupants.
        for (int i = 0; i < 7; i++) {
            Compaction.delete(t, new byte[]{(byte) i});
        }
        int changes = Compaction.compact(t);
        assertTrue(changes >= 1, "expected at least one shape change");
        // Surviving keys still resolvable.
        for (int i = 7; i < 10; i++) {
            assertEquals(i, t.get(new byte[]{(byte) i}));
        }
        // Deleted keys no longer return.
        for (int i = 0; i < 7; i++) {
            assertNull(t.get(new byte[]{(byte) i}));
        }
    }

    @Test
    void compactIsIdempotent() {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 10; i++) {
            t.insert(new byte[]{(byte) i}, i);
        }
        for (int i = 0; i < 7; i++) {
            Compaction.delete(t, new byte[]{(byte) i});
        }
        int first = Compaction.compact(t);
        int second = Compaction.compact(t);
        assertTrue(first >= 1);
        assertEquals(0, second, "no further compaction; got " + second + " changes");
        for (int i = 7; i < 10; i++) {
            assertEquals(i, t.get(new byte[]{(byte) i}));
        }
    }

    @Test
    void compactPrunesEmptySubtrees() {
        Art<Integer> t = new Art<>();
        // Insert deep paths then delete one terminal value.
        t.insert("hello".getBytes(), 1);
        t.insert("world".getBytes(), 2);
        assertEquals(1, Compaction.delete(t, "hello".getBytes()));
        int changes = Compaction.compact(t);
        assertTrue(changes > 0, "pruning should report changes");
        assertEquals(2, t.get("world".getBytes()));
        assertNull(t.get("hello".getBytes()));
        // Idempotent.
        assertEquals(0, Compaction.compact(t));
    }

    @Test
    void compactOnEmptyTreeIsNoop() {
        Art<Integer> t = new Art<>();
        assertEquals(0, Compaction.compact(t));
        assertEquals(0, t.size());
    }

    @Test
    void compactKeepsFullWhenOccupancyAboveFour() {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 10; i++) {
            t.insert(new byte[]{(byte) i}, i);
        }
        Compaction.delete(t, new byte[]{(byte) 0});
        Compaction.delete(t, new byte[]{(byte) 1});
        Compaction.compact(t);
        for (int i = 2; i < 10; i++) {
            assertEquals(i, t.get(new byte[]{(byte) i}));
        }
    }

    @Test
    void deleteOfMissingKeyReturnsNull() {
        Art<Integer> t = new Art<>();
        t.insert("present".getBytes(), 1);
        assertNull(Compaction.delete(t, "absent".getBytes()));
        assertEquals(1, t.size());
        assertNull(Compaction.delete(t, "nope_no_path".getBytes()));
    }
}
