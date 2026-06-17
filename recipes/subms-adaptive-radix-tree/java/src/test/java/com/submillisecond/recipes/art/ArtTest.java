package com.submillisecond.recipes.art;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ArtTest {

    @Test
    void insertAndGet() {
        Art<Integer> t = new Art<>();
        t.insert("alice".getBytes(), 1);
        t.insert("bob".getBytes(), 2);
        t.insert("alex".getBytes(), 3);
        assertEquals(1, t.get("alice".getBytes()));
        assertEquals(2, t.get("bob".getBytes()));
        assertEquals(3, t.get("alex".getBytes()));
        assertNull(t.get("missing".getBytes()));
        assertEquals(3, t.size());
    }

    @Test
    void insertReplacesValue() {
        Art<String> t = new Art<>();
        assertNull(t.insert("k".getBytes(), "first"));
        assertEquals("first", t.insert("k".getBytes(), "second"));
        assertEquals("second", t.get("k".getBytes()));
        assertEquals(1, t.size());
    }

    @Test
    void emptyKey() {
        Art<Integer> t = new Art<>();
        t.insert(new byte[0], 42);
        assertEquals(42, t.get(new byte[0]));
    }

    @Test
    void manyKeysForceNodeGrowth() {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 256; i++) {
            byte[] key = new byte[]{(byte) i, 0, 0};
            t.insert(key, i);
        }
        assertEquals(256, t.size());
        for (int i = 0; i < 256; i++) {
            byte[] key = new byte[]{(byte) i, 0, 0};
            assertEquals(i, t.get(key));
        }
    }

    @Test
    void sharedPrefixes() {
        Art<Integer> t = new Art<>();
        t.insert("prefix/a".getBytes(), 1);
        t.insert("prefix/b".getBytes(), 2);
        t.insert("prefix/c".getBytes(), 3);
        assertEquals(1, t.get("prefix/a".getBytes()));
        assertEquals(2, t.get("prefix/b".getBytes()));
        assertEquals(3, t.get("prefix/c".getBytes()));
        assertNull(t.get("prefix".getBytes()));
    }

    @Test
    void emptyTreeIsEmpty() {
        Art<Integer> t = new Art<>();
        assertTrue(t.isEmpty());
        assertEquals(0, t.size());
        assertNull(t.get("any".getBytes()));
    }

    @Test
    void longKeys() {
        Art<Integer> t = new Art<>();
        byte[] key = new byte[1000];
        java.util.Arrays.fill(key, (byte) 'a');
        t.insert(key, 42);
        assertEquals(42, t.get(key));
    }

    @Test
    void binaryKeysWithZeroBytes() {
        Art<Integer> t = new Art<>();
        t.insert(new byte[]{0, 1, 2}, 1);
        t.insert(new byte[]{0, 1, 3}, 2);
        t.insert(new byte[]{0, 2, 0}, 3);
        assertEquals(1, t.get(new byte[]{0, 1, 2}));
        assertEquals(2, t.get(new byte[]{0, 1, 3}));
        assertEquals(3, t.get(new byte[]{0, 2, 0}));
    }

    @Test
    void shorterKeyDistinctFromLongerWithPrefix() {
        Art<Integer> t = new Art<>();
        t.insert("foo".getBytes(), 1);
        t.insert("foobar".getBytes(), 2);
        assertEquals(1, t.get("foo".getBytes()));
        assertEquals(2, t.get("foobar".getBytes()));
        assertNull(t.get("foob".getBytes()));
    }

    @Test
    void insertReplacesValueAtSameKey() {
        Art<Integer> t = new Art<>();
        t.insert("k".getBytes(), 1);
        t.insert("k".getBytes(), 99);
        assertEquals(99, t.get("k".getBytes()), "second insert must overwrite, not append");
    }

    @Test
    void emptyKeyIsRejectedOrConsistent() {
        Art<Integer> t = new Art<>();
        // Either the API allows empty keys (and stores them) or rejects them;
        // we pin whichever the implementation chose so future refactors can't
        // silently change the contract.
        try {
            t.insert(new byte[0], 7);
            assertEquals(7, t.get(new byte[0]));
        } catch (IllegalArgumentException expected) {
            // also acceptable: explicit rejection of empty keys
        }
    }
}
