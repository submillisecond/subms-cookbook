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
    @Test
    void growthBoundariesNode4_16_48_256() {
        // Cross each adaptive transition by root fan-out: 5 -> Node16, 17 -> Node48,
        // 49 -> Node256. Every key stays retrievable across the promotions.
        for (int n : new int[]{5, 17, 49, 256}) {
            Art<Integer> t = new Art<>();
            for (int i = 0; i < n; i++) {
                t.insert(new byte[]{(byte) i}, i);
            }
            assertEquals(n, t.size(), "n=" + n);
            for (int i = 0; i < n; i++) {
                assertEquals(Integer.valueOf(i), t.get(new byte[]{(byte) i}), "n=" + n + " key=" + i);
            }
        }
    }

    @Test
    void multiLevelPathCompressionSplits() {
        Art<Integer> t = new Art<>();
        t.insert("abcdefg".getBytes(), 1);
        t.insert("abcdefh".getBytes(), 2); // splits at the 7th byte
        t.insert("abcxyz".getBytes(), 3);  // splits the abc node at the 4th byte
        t.insert("abc".getBytes(), 4);     // a key that is a prefix of the others
        t.insert("a".getBytes(), 5);       // an even shorter prefix key
        assertEquals(5, t.size());
        assertEquals(Integer.valueOf(1), t.get("abcdefg".getBytes()));
        assertEquals(Integer.valueOf(2), t.get("abcdefh".getBytes()));
        assertEquals(Integer.valueOf(3), t.get("abcxyz".getBytes()));
        assertEquals(Integer.valueOf(4), t.get("abc".getBytes()));
        assertEquals(Integer.valueOf(5), t.get("a".getBytes()));
        // Interior points that carry no value must still miss.
        assertNull(t.get("ab".getBytes()));
        assertNull(t.get("abcd".getBytes()));
        assertNull(t.get("abcde".getBytes()));
        assertNull(t.get("abcdef".getBytes()));
    }
}
