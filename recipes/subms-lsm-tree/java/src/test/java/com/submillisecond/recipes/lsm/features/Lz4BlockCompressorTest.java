package com.submillisecond.recipes.lsm.features;

import org.junit.jupiter.api.Test;

import java.io.IOException;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class Lz4BlockCompressorTest {

    private static byte[] repetitive(int n) {
        byte[] pattern = "the-quick-brown-fox-jumps-over-the-lazy-dog-".getBytes();
        byte[] out = new byte[n];
        for (int i = 0; i < n; i++) out[i] = pattern[i % pattern.length];
        return out;
    }

    @Test
    void roundTripCompressible() throws IOException {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        byte[] input = repetitive(4096);
        byte[] enc = c.compress(input);
        assertTrue(enc.length < input.length, "compressible payload should shrink");
        assertArrayEquals(input, c.decompress(enc));
    }

    @Test
    void roundTripEmpty() throws IOException {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        byte[] enc = c.compress(new byte[0]);
        assertArrayEquals(new byte[0], c.decompress(enc));
    }

    @Test
    void incompressibleFallsBackToStored() throws IOException {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        // Pseudorandom-but-deterministic xorshift bytes - LZ4 won't shrink these.
        int s = 0x9E3779B9;
        byte[] input = new byte[2048];
        for (int i = 0; i < input.length; i++) {
            s ^= s << 13;
            s ^= s >>> 17;
            s ^= s << 5;
            input[i] = (byte) s;
        }
        byte[] enc = c.compress(input);
        assertEquals(0x00, enc[1], "incompressible falls back to stored");
        assertArrayEquals(input, c.decompress(enc));
    }

    @Test
    void badMarkerErrors() {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        byte[] enc = c.compress("hello world hello world hello world".getBytes());
        enc[0] = (byte) 0xff;
        assertThrows(IOException.class, () -> c.decompress(enc));
    }

    @Test
    void tooShortBufferErrors() {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        assertThrows(IOException.class, () -> c.decompress(new byte[3]));
    }

    @Test
    void unknownAlgoByteErrors() {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        byte[] enc = c.compress("abcdef".getBytes());
        enc[1] = 0x7f;
        assertThrows(IOException.class, () -> c.decompress(enc));
    }
}
