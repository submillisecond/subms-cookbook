package com.submillisecond.recipes.art.features;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.art.Art;

/**
 * The SAME four keys, the SAME bytes, in both ports.
 *
 * <p>Pinned as a hex literal rather than a round-trip, because a round-trip only
 * proves a port agrees with ITSELF - which is exactly what let this port ship v1
 * (one record per key byte) while Rust shipped v2 (one record per node), each
 * passing its own suite, while this class's own javadoc claimed the two were
 * byte-equivalent on disk. They were mutually unreadable.
 *
 * <p>Generated from the real writers. If this fails, the wire format moved.
 */
final class SerializeCrossPortTest {

    private static final String CROSS_PORT_FIXTURE =
            "4152546200020000000000000000000400000200026100026c7002000265000272740100000001326800016101000000013162000003000000013400016500027461010000000133";

    private static Art<byte[]> sample() {
        Art<byte[]> t = new Art<>();
        t.insert("alpha".getBytes(), "1".getBytes());
        t.insert("alpert".getBytes(), "2".getBytes());
        t.insert("beta".getBytes(), "3".getBytes());
        t.insert("b".getBytes(), "4".getBytes());
        return t;
    }

    private static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(String.format("%02x", b));
        }
        return sb.toString();
    }

    private static byte[] unhex(String s) {
        byte[] out = new byte[s.length() / 2];
        for (int i = 0; i < out.length; i++) {
            out[i] = (byte) Integer.parseInt(s.substring(i * 2, i * 2 + 2), 16);
        }
        return out;
    }

    @Test
    void wireFormatMatchesTheRustPortByteForByte() throws Exception {
        assertEquals(CROSS_PORT_FIXTURE, hex(Serialize.writeToBytes(sample(), Serialize.BYTES)));
    }

    @Test
    void aRustWrittenStreamDecodesHere() throws Exception {
        Art<byte[]> tree = Serialize.parseBytes(unhex(CROSS_PORT_FIXTURE), Serialize.BYTES);
        assertEquals(4, tree.size());
        assertArrayEquals("1".getBytes(), tree.get("alpha".getBytes()));
        assertArrayEquals("2".getBytes(), tree.get("alpert".getBytes()));
        assertArrayEquals("3".getBytes(), tree.get("beta".getBytes()));
        assertArrayEquals("4".getBytes(), tree.get("b".getBytes()));
    }
}
