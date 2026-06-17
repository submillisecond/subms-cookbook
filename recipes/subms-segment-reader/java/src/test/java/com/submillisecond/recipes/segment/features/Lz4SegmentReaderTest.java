package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class Lz4SegmentReaderTest {

    private static byte[] writeOne(byte[] payload) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Lz4BlockWriter(dos).write(payload);
        dos.close();
        return baos.toByteArray();
    }

    private static Lz4SegmentReader open(byte[] bytes) {
        return new Lz4SegmentReader(new DataInputStream(new ByteArrayInputStream(bytes)));
    }

    @Test
    void roundTripCompressiblePayload() throws Exception {
        byte[] payload = new byte[4096];
        Arrays.fill(payload, (byte) 'a');
        byte[] data = writeOne(payload);
        // LZ4 path expected for highly compressible input.
        assertTrue(data.length < payload.length, "lz4 path expected, got size=" + data.length);
        Lz4SegmentReader r = open(data);
        assertArrayEquals(payload, r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void roundTripStoredPathForIncompressiblePayload() throws Exception {
        // Pseudo-random-ish bytes: LZ4 would inflate; writer picks stored.
        byte[] payload = new byte[64];
        for (int i = 0; i < 64; i++) {
            payload[i] = (byte) ((i * 2654435761L) >>> 24);
        }
        byte[] data = writeOne(payload);
        assertEquals(Lz4SegmentReader.TAG_STORED, data[0], "stored path expected when lz4 would inflate");
        assertEquals(9 + payload.length, data.length);
        Lz4SegmentReader r = open(data);
        assertArrayEquals(payload, r.nextRecord());
    }

    @Test
    void emptySegmentYieldsNull() throws Exception {
        assertNull(open(new byte[0]).nextRecord());
    }

    @Test
    void unknownAlgoTagRejected() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(99);
        dos.writeInt(5);
        dos.writeInt(5);
        dos.write("hello".getBytes());
        dos.close();
        Lz4SegmentReader r = open(baos.toByteArray());
        assertThrows(Lz4SegmentReader.DecompressionFailed.class, r::nextRecord);
    }

    @Test
    void truncatedHeaderSurfacesTypedError() throws Exception {
        byte[] hand = new byte[]{Lz4SegmentReader.TAG_STORED, 0, 0, 0}; // 3 of 8 trailing header bytes
        Lz4SegmentReader r = open(hand);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void truncatedPayloadSurfacesTypedError() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(Lz4SegmentReader.TAG_STORED);
        dos.writeInt(10);
        dos.writeInt(10);
        dos.write("abc".getBytes()); // only 3 of 10 actual bytes
        dos.close();
        Lz4SegmentReader r = open(baos.toByteArray());
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void corruptedLz4PayloadReturnsDecompressionFailed() throws Exception {
        byte[] payload = new byte[4096];
        Arrays.fill(payload, (byte) 'a');
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Lz4BlockWriter(dos).writeLz4(payload);
        dos.close();
        byte[] data = baos.toByteArray();
        // Smash bytes deep in the compressed body to derail the lz4 state machine.
        data[data.length - 4] ^= (byte) 0xff;
        data[data.length - 5] ^= (byte) 0xff;
        data[data.length - 6] ^= (byte) 0xff;
        Lz4SegmentReader r = open(data);
        try {
            byte[] out = r.nextRecord();
            // If it didn't throw, the round-trip must at minimum not match.
            assertTrue(!Arrays.equals(payload, out), "corrupted lz4 must not round-trip");
        } catch (Lz4SegmentReader.DecompressionFailed expected) {
            // Acceptable: detected outright.
        }
    }

    @Test
    void storedBlockSizeMismatchRejected() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(Lz4SegmentReader.TAG_STORED);
        dos.writeInt(10);
        dos.writeInt(8);
        dos.write("12345678".getBytes());
        dos.close();
        Lz4SegmentReader r = open(baos.toByteArray());
        assertThrows(Lz4SegmentReader.DecompressionFailed.class, r::nextRecord);
    }

    @Test
    void negativeBlockLengthRejected() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(Lz4SegmentReader.TAG_STORED);
        dos.writeInt(-1);
        dos.writeInt(5);
        dos.write("hello".getBytes());
        dos.close();
        Lz4SegmentReader r = open(baos.toByteArray());
        assertThrows(java.io.IOException.class, r::nextRecord);
    }

    @Test
    void writeStoredRoundTrip() throws Exception {
        byte[] payload = "stored-direct".getBytes();
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Lz4BlockWriter(dos).writeStored(payload);
        dos.close();
        byte[] data = baos.toByteArray();
        assertEquals(Lz4SegmentReader.TAG_STORED, data[0]);
        Lz4SegmentReader r = open(data);
        assertArrayEquals(payload, r.nextRecord());
    }

    @Test
    void multipleBlocksRoundTrip() throws Exception {
        byte[] big = new byte[1024];
        Arrays.fill(big, (byte) 'x');
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Lz4BlockWriter w = new Lz4BlockWriter(dos);
        w.write(big);
        w.write("small".getBytes());
        byte[] big2 = new byte[2048];
        Arrays.fill(big2, (byte) 'z');
        w.write(big2);
        dos.close();
        Lz4SegmentReader r = open(baos.toByteArray());
        assertArrayEquals(big, r.nextRecord());
        assertArrayEquals("small".getBytes(), r.nextRecord());
        assertArrayEquals(big2, r.nextRecord());
        assertNull(r.nextRecord());
    }
}
