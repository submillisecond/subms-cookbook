package com.submillisecond.recipes.lsm.features;

import org.junit.jupiter.api.Test;

import java.io.IOException;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ZstdBlockCompressorTest {

    private static byte[] repetitive(int n) {
        byte[] pattern = "the-quick-brown-fox-jumps-over-the-lazy-dog-".getBytes();
        byte[] out = new byte[n];
        for (int i = 0; i < n; i++) out[i] = pattern[i % pattern.length];
        return out;
    }

    @Test
    void roundTripCompressible() throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] input = repetitive(4096);
        byte[] enc = c.compress(input);
        assertTrue(enc.length < input.length, "compressible payload should shrink");
        assertArrayEquals(input, c.decompress(enc));
    }

    @Test
    void roundTripEmpty() throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] enc = c.compress(new byte[0]);
        assertArrayEquals(new byte[0], c.decompress(enc));
    }

    @Test
    void levelIsClampedToValidRange() {
        assertEquals(1, ZstdBlockCompressor.withLevel(-99).level());
        assertEquals(22, ZstdBlockCompressor.withLevel(99).level());
        assertEquals(10, ZstdBlockCompressor.withLevel(10).level());
    }

    @Test
    void incompressibleFallsBackToStored() throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        int s = 0xDEADBEEF;
        byte[] input = new byte[64];
        for (int i = 0; i < input.length; i++) {
            s ^= s << 13;
            s ^= s >>> 17;
            s ^= s << 5;
            input[i] = (byte) s;
        }
        byte[] enc = c.compress(input);
        assertEquals(0x00, enc[1], "incompressible takes the stored path");
        assertArrayEquals(input, c.decompress(enc));
    }

    @Test
    void badMarkerErrors() throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] enc = c.compress("hello world hello world hello world".getBytes());
        enc[0] = 0x00;
        assertThrows(IOException.class, () -> c.decompress(enc));
    }

    @Test
    void tooShortBufferErrors() {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        assertThrows(IOException.class, () -> c.decompress(new byte[3]));
    }

    @Test
    void unknownAlgoByteErrors() throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] enc = c.compress("abcdefghij".getBytes());
        enc[1] = 0x77;
        assertThrows(IOException.class, () -> c.decompress(enc));
    }
}
