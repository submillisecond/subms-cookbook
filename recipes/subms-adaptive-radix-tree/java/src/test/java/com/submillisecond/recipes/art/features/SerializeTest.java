package com.submillisecond.recipes.art.features;

import java.io.IOException;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.art.Art;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SerializeTest {

    @Test
    void roundTripEmptyTree() throws IOException {
        Art<Integer> t = new Art<>();
        byte[] data = Serialize.writeToBytes(t, Serialize.INT32);
        Art<Integer> restored = Serialize.parseBytes(data, Serialize.INT32);
        assertEquals(0, restored.size());
        assertTrue(restored.isEmpty());
    }

    @Test
    void roundTripPreservesAllInsertions() throws IOException {
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 200; i++) {
            String k = String.format("key%04d", i);
            t.insert(k.getBytes(), i);
        }
        byte[] data = Serialize.writeToBytes(t, Serialize.INT32);
        Art<Integer> restored = Serialize.parseBytes(data, Serialize.INT32);
        assertEquals(200, restored.size());
        for (int i = 0; i < 200; i++) {
            String k = String.format("key%04d", i);
            assertEquals(i, restored.get(k.getBytes()), "key " + k);
        }
    }

    @Test
    void roundTripWithNodeGrowth() throws IOException {
        // 256 distinct first bytes force the root from Small -> Full.
        Art<Integer> t = new Art<>();
        for (int i = 0; i < 256; i++) {
            byte[] key = new byte[]{(byte) i, 7, 9};
            t.insert(key, i);
        }
        byte[] data = Serialize.writeToBytes(t, Serialize.INT32);
        Art<Integer> restored = Serialize.parseBytes(data, Serialize.INT32);
        assertEquals(256, restored.size());
        for (int i = 0; i < 256; i++) {
            byte[] key = new byte[]{(byte) i, 7, 9};
            assertEquals(i, restored.get(key));
        }
    }

    @Test
    void roundTripWithStringValues() throws IOException {
        Art<String> t = new Art<>();
        t.insert("a".getBytes(), "alpha");
        t.insert("b".getBytes(), "beta");
        t.insert("long-key-with-many-bytes".getBytes(), "gamma");
        byte[] data = Serialize.writeToBytes(t, Serialize.STRING);
        Art<String> restored = Serialize.parseBytes(data, Serialize.STRING);
        assertEquals("alpha", restored.get("a".getBytes()));
        assertEquals("beta", restored.get("b".getBytes()));
        assertEquals("gamma", restored.get("long-key-with-many-bytes".getBytes()));
    }

    @Test
    void roundTripBinaryKeysWithZeros() throws IOException {
        Art<Integer> t = new Art<>();
        t.insert(new byte[]{0, 0, 0}, 1);
        t.insert(new byte[]{0, 1, 2}, 2);
        t.insert(new byte[0], 99);
        byte[] data = Serialize.writeToBytes(t, Serialize.INT32);
        Art<Integer> restored = Serialize.parseBytes(data, Serialize.INT32);
        assertEquals(1, restored.get(new byte[]{0, 0, 0}));
        assertEquals(2, restored.get(new byte[]{0, 1, 2}));
        assertEquals(99, restored.get(new byte[0]));
        assertEquals(3, restored.size());
    }

    @Test
    void badMagicIsRejected() {
        byte[] bad = new byte[16];
        // first 4 bytes are not ARTb
        bad[5] = 1;
        assertThrows(IOException.class, () -> Serialize.parseBytes(bad, Serialize.INT32));
    }

    @Test
    void badVersionIsRejected() {
        // Build a header with the right magic but a junk version.
        byte[] header = new byte[16];
        header[0] = 'A';
        header[1] = 'R';
        header[2] = 'T';
        header[3] = 'b';
        // version = 999 (big-endian short)
        header[4] = (byte) ((999 >> 8) & 0xff);
        header[5] = (byte) (999 & 0xff);
        assertThrows(IOException.class, () -> Serialize.parseBytes(header, Serialize.INT32));
    }

    @Test
    void byteArrayValueRoundTrip() throws IOException {
        Art<byte[]> t = new Art<>();
        t.insert("a".getBytes(), new byte[]{1, 2, 3});
        t.insert("b".getBytes(), new byte[]{});
        byte[] data = Serialize.writeToBytes(t, Serialize.BYTES);
        Art<byte[]> restored = Serialize.parseBytes(data, Serialize.BYTES);
        byte[] got = restored.get("a".getBytes());
        assertNotNull(got);
        assertArrayEquals(new byte[]{1, 2, 3}, got);
        assertArrayEquals(new byte[]{}, restored.get("b".getBytes()));
        assertNull(restored.get("missing".getBytes()));
    }
}
