package com.submillisecond.recipes.segment;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class SegmentReaderTest {

    private static byte[] build(byte[]... records) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (byte[] r : records) w.write(r);
        dos.close();
        return baos.toByteArray();
    }

    private static SegmentReader open(byte[] bytes) {
        return new SegmentReader(new DataInputStream(new ByteArrayInputStream(bytes)));
    }

    @Test
    void roundTripSingleRecord() throws Exception {
        byte[] data = build("hello".getBytes());
        SegmentReader r = open(data);
        assertArrayEquals("hello".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void roundTripMultiple() throws Exception {
        byte[] data = build("alice".getBytes(), "bob".getBytes(), "carol".getBytes());
        SegmentReader r = open(data);
        assertArrayEquals("alice".getBytes(), r.nextRecord());
        assertArrayEquals("bob".getBytes(), r.nextRecord());
        assertArrayEquals("carol".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void emptyYieldsNull() throws Exception {
        assertNull(open(new byte[0]).nextRecord());
    }

    @Test
    void truncatedPayloadThrowsTyped() throws Exception {
        byte[] full = build("first".getBytes());
        byte[] hand = new byte[full.length + 8];
        System.arraycopy(full, 0, hand, 0, full.length);
        // Header claiming 10 bytes follow, but only 3 written.
        hand[full.length    ] = 0; hand[full.length + 1] = 0;
        hand[full.length + 2] = 0; hand[full.length + 3] = 10;
        hand[full.length + 4] = 'a'; hand[full.length + 5] = 'b'; hand[full.length + 6] = 'c';
        SegmentReader r = open(java.util.Arrays.copyOf(hand, full.length + 7));
        assertArrayEquals("first".getBytes(), r.nextRecord());
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void zeroLengthRecord() throws Exception {
        byte[] data = build(new byte[0], "after-empty".getBytes());
        SegmentReader r = open(data);
        assertArrayEquals(new byte[0], r.nextRecord());
        assertArrayEquals("after-empty".getBytes(), r.nextRecord());
    }

    @Test
    void largeRecordRoundTrip() throws Exception {
        byte[] big = new byte[8192];
        java.util.Arrays.fill(big, (byte) 0xab);
        byte[] data = build(big);
        SegmentReader r = open(data);
        assertArrayEquals(big, r.nextRecord());
    }

    @Test
    void manyRecordsInARow() throws Exception {
        byte[][] records = new byte[1000][];
        for (int i = 0; i < 1000; i++) records[i] = ("rec-" + i).getBytes();
        byte[] data = build(records);
        SegmentReader r = open(data);
        for (byte[] expected : records) assertArrayEquals(expected, r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void writerCallableMultipleTimes() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (int i = 0; i < 10; i++) w.write(("r" + i).getBytes());
        dos.close();
        SegmentReader r = open(baos.toByteArray());
        for (int i = 0; i < 10; i++) assertArrayEquals(("r" + i).getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }
}
