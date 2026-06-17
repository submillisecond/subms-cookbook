package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class Crc32SegmentReaderTest {

    private static byte[] build(byte[]... records) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Crc32SegmentWriter w = new Crc32SegmentWriter(dos);
        for (byte[] r : records) w.write(r);
        dos.close();
        return baos.toByteArray();
    }

    private static Crc32SegmentReader open(byte[] bytes) {
        return new Crc32SegmentReader(new DataInputStream(new ByteArrayInputStream(bytes)));
    }

    @Test
    void roundTripWithChecksum() throws Exception {
        byte[] data = build("alice".getBytes(), "bob".getBytes(), "carol".getBytes());
        Crc32SegmentReader r = open(data);
        assertArrayEquals("alice".getBytes(), r.nextRecord());
        assertArrayEquals("bob".getBytes(), r.nextRecord());
        assertArrayEquals("carol".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void emptySegmentYieldsNull() throws Exception {
        assertNull(open(new byte[0]).nextRecord());
    }

    @Test
    void corruptedPayloadDetected() throws Exception {
        byte[] data = build("hello".getBytes());
        data[4] ^= (byte) 0x80; // flip a bit in the payload
        Crc32SegmentReader r = open(data);
        assertThrows(Crc32SegmentReader.ChecksumMismatch.class, r::nextRecord);
    }

    @Test
    void corruptedTrailerDetected() throws Exception {
        byte[] data = build("hello".getBytes());
        data[data.length - 1] ^= (byte) 0xff;
        Crc32SegmentReader r = open(data);
        assertThrows(Crc32SegmentReader.ChecksumMismatch.class, r::nextRecord);
    }

    @Test
    void truncatedTrailerSurfacesTypedError() throws Exception {
        byte[] data = build("hello".getBytes());
        byte[] chopped = new byte[data.length - 2];
        System.arraycopy(data, 0, chopped, 0, chopped.length);
        Crc32SegmentReader r = open(chopped);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void truncatedPayloadSurfacesTypedError() throws Exception {
        // Header claims 10 bytes follow but only 3 actually do.
        byte[] hand = new byte[]{0, 0, 0, 10, 'a', 'b', 'c'};
        Crc32SegmentReader r = open(hand);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void zeroLengthRecordRoundTrips() throws Exception {
        byte[] data = build(new byte[0], "after-empty".getBytes());
        Crc32SegmentReader r = open(data);
        assertArrayEquals(new byte[0], r.nextRecord());
        assertArrayEquals("after-empty".getBytes(), r.nextRecord());
    }

    @Test
    void negativeFrameLengthRejected() throws Exception {
        byte[] hand = new byte[]{(byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff};
        Crc32SegmentReader r = open(hand);
        assertThrows(java.io.IOException.class, r::nextRecord);
    }
}
