package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.util.HashSet;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class Xxh3SegmentReaderTest {

    private static byte[] build(byte[]... records) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Xxh3SegmentWriter w = new Xxh3SegmentWriter(dos);
        for (byte[] r : records) w.write(r);
        dos.close();
        return baos.toByteArray();
    }

    private static Xxh3SegmentReader open(byte[] bytes) {
        return new Xxh3SegmentReader(new DataInputStream(new ByteArrayInputStream(bytes)));
    }

    @Test
    void roundTripWithHash() throws Exception {
        byte[] data = build("alpha".getBytes(), "beta".getBytes(), "gamma".getBytes());
        Xxh3SegmentReader r = open(data);
        assertArrayEquals("alpha".getBytes(), r.nextRecord());
        assertArrayEquals("beta".getBytes(), r.nextRecord());
        assertArrayEquals("gamma".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void emptySegmentYieldsNull() throws Exception {
        assertNull(open(new byte[0]).nextRecord());
    }

    @Test
    void corruptedPayloadDetected() throws Exception {
        byte[] data = build("hello".getBytes());
        data[4] ^= (byte) 0x40;
        Xxh3SegmentReader r = open(data);
        assertThrows(Xxh3SegmentReader.ChecksumMismatch.class, r::nextRecord);
    }

    @Test
    void corruptedTrailerDetected() throws Exception {
        byte[] data = build("hello".getBytes());
        data[data.length - 1] ^= (byte) 0xff;
        Xxh3SegmentReader r = open(data);
        assertThrows(Xxh3SegmentReader.ChecksumMismatch.class, r::nextRecord);
    }

    @Test
    void truncatedTrailerSurfacesTypedError() throws Exception {
        byte[] data = build("hello".getBytes());
        // Trailer is 8 bytes; chop 4.
        byte[] chopped = new byte[data.length - 4];
        System.arraycopy(data, 0, chopped, 0, chopped.length);
        Xxh3SegmentReader r = open(chopped);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void truncatedPayloadSurfacesTypedError() throws Exception {
        byte[] hand = new byte[]{0, 0, 0, 10, 'a', 'b', 'c'};
        Xxh3SegmentReader r = open(hand);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void zeroLengthRecordRoundTrips() throws Exception {
        byte[] data = build(new byte[0], "after-empty".getBytes());
        Xxh3SegmentReader r = open(data);
        assertArrayEquals(new byte[0], r.nextRecord());
        assertArrayEquals("after-empty".getBytes(), r.nextRecord());
    }

    @Test
    void hashIsLengthSensitive() {
        // Same prefix, different lengths -> different hash.
        byte[] a = "hello".getBytes();
        byte[] b = "helloo".getBytes();
        assertNotEquals(Xxh3.hash64(a, 0, a.length), Xxh3.hash64(b, 0, b.length));
    }

    @Test
    void hashAvoidsTrivialCollisionsOnSmallInputs() {
        // Probe a few hundred small inputs - all hashes must be distinct.
        HashSet<Long> seen = new HashSet<>();
        for (int i = 0; i < 500; i++) {
            byte[] s = ("rec-" + i).getBytes();
            long h = Xxh3.hash64(s, 0, s.length);
            assertTrue(seen.add(h), "collision at i=" + i);
        }
    }

    @Test
    void negativeLengthRejected() {
        assertThrows(IllegalArgumentException.class, () -> Xxh3.hash64(new byte[0], 0, -1));
    }

    @Test
    void hashHandlesLargeInputAcrossAccumulatorPath() throws Exception {
        // >= 32 bytes triggers the 4-accumulator main loop. 100 bytes
        // also exercises the 8-byte tail loop and the 4-byte tail.
        byte[] big = new byte[100];
        for (int i = 0; i < big.length; i++) big[i] = (byte) i;
        long h = Xxh3.hash64(big, 0, big.length);
        // Round-trip via the writer/reader, too.
        byte[] data = build(big);
        Xxh3SegmentReader r = open(data);
        assertArrayEquals(big, r.nextRecord());
        // Bumping any single byte should change the hash.
        big[50] ^= (byte) 0x01;
        long h2 = Xxh3.hash64(big, 0, big.length);
        assertNotEquals(h, h2);
    }

    @Test
    void hashHandlesExactBlockSizes() {
        // 32 bytes -> exactly one accumulator pass, no tail.
        // 36 bytes -> one accumulator pass + 4-byte tail.
        // 40 bytes -> one accumulator pass + 8-byte tail.
        byte[] b32 = new byte[32];
        byte[] b36 = new byte[36];
        byte[] b40 = new byte[40];
        for (int i = 0; i < 40; i++) {
            byte v = (byte) (i + 1);
            if (i < 32) b32[i] = v;
            if (i < 36) b36[i] = v;
            b40[i] = v;
        }
        long h32 = Xxh3.hash64(b32, 0, 32);
        long h36 = Xxh3.hash64(b36, 0, 36);
        long h40 = Xxh3.hash64(b40, 0, 40);
        // Three different lengths -> three different hashes.
        assertNotEquals(h32, h36);
        assertNotEquals(h36, h40);
        assertNotEquals(h32, h40);
    }

    @Test
    void negativeFrameLengthRejected() throws Exception {
        byte[] hand = new byte[]{(byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff};
        Xxh3SegmentReader r = open(hand);
        assertThrows(java.io.IOException.class, r::nextRecord);
    }

    @Test
    void hashHandlesOffsetVariant() {
        // Cover the (data, off, len) overload directly.
        byte[] all = "PREFIXhelloSUFFIX".getBytes();
        long h1 = Xxh3.hash64("hello".getBytes(), 0, 5);
        long h2 = Xxh3.hash64(all, 6, 5);
        assertEquals(h1, h2, "offset-based hash must match aligned-hash of the same window");
    }
}
