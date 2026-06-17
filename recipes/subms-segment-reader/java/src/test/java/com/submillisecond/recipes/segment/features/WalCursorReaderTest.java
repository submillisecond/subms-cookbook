package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import com.submillisecond.recipes.segment.SegmentWriter;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class WalCursorReaderTest {

    private static byte[] build(byte[]... records) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (byte[] r : records) w.write(r);
        dos.close();
        return baos.toByteArray();
    }

    /** Byte offset of the tail of block {@code n} (0-indexed). */
    private static int endOfBlock(byte[] buf, int n) {
        int pos = 0;
        for (int i = 0; i <= n; i++) {
            int len = ((buf[pos] & 0xff) << 24)
                    | ((buf[pos + 1] & 0xff) << 16)
                    | ((buf[pos + 2] & 0xff) << 8)
                    | (buf[pos + 3] & 0xff);
            pos += 4 + len;
        }
        return pos;
    }

    @Test
    void nothingVisibleUntilWatermarkAdvances() throws Exception {
        WalCursorReader r = new WalCursorReader(build("a".getBytes(), "b".getBytes(), "c".getBytes()));
        assertNull(r.readCommitted(), "watermark at 0 -> nothing visible");
    }

    @Test
    void advanceToFirstBlockExposesFirstBlockOnly() throws Exception {
        byte[] data = build("a".getBytes(), "b".getBytes(), "c".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        r.setCommitted(endOfBlock(data, 0));
        assertArrayEquals("a".getBytes(), r.readCommitted());
        assertNull(r.readCommitted(), "second block not yet committed");
    }

    @Test
    void advanceAgainExposesMoreBlocks() throws Exception {
        byte[] data = build("a".getBytes(), "b".getBytes(), "c".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        r.setCommitted(endOfBlock(data, 0));
        r.readCommitted();
        r.setCommitted(endOfBlock(data, 2));
        assertArrayEquals("b".getBytes(), r.readCommitted());
        assertArrayEquals("c".getBytes(), r.readCommitted());
        assertNull(r.readCommitted());
    }

    @Test
    void watermarkIsMonotonic() throws Exception {
        byte[] data = build("a".getBytes(), "b".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        int high = endOfBlock(data, 1);
        r.setCommitted(high);
        assertEquals(high, r.committed());
        r.setCommitted(0);
        assertEquals(high, r.committed(), "backward moves rejected");
    }

    @Test
    void watermarkClampsToBufferLength() throws Exception {
        byte[] data = build("a".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        r.setCommitted(9999);
        assertEquals(data.length, r.committed());
    }

    @Test
    void withCommittedSeedsWatermarkAtOpen() throws Exception {
        byte[] data = build("a".getBytes(), "b".getBytes());
        WalCursorReader r = new WalCursorReader(data, endOfBlock(data, 0));
        assertArrayEquals("a".getBytes(), r.readCommitted());
        assertNull(r.readCommitted());
    }

    @Test
    void dirtyNextRecordIgnoresWatermark() throws Exception {
        byte[] data = build("a".getBytes(), "b".getBytes());
        WalCursorReader r = new WalCursorReader(data); // committed = 0
        assertArrayEquals("a".getBytes(), r.nextRecord());
        assertArrayEquals("b".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void truncatedTailSurfacesTypedError() throws Exception {
        byte[] base = build("first".getBytes());
        byte[] hand = new byte[base.length + 7];
        System.arraycopy(base, 0, hand, 0, base.length);
        // Header claiming 10 bytes follow but only 3 actually do.
        hand[base.length] = 0; hand[base.length + 1] = 0;
        hand[base.length + 2] = 0; hand[base.length + 3] = 10;
        hand[base.length + 4] = 'a'; hand[base.length + 5] = 'b'; hand[base.length + 6] = 'c';
        WalCursorReader r = new WalCursorReader(hand);
        r.setCommitted(hand.length);
        assertArrayEquals("first".getBytes(), r.readCommitted());
        assertThrows(SegmentReader.TruncatedFrame.class, r::readCommitted);
    }

    @Test
    void positionAdvancesWithReads() throws Exception {
        byte[] data = build("a".getBytes(), "bb".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        r.setCommitted(data.length);
        assertEquals(0, r.position());
        r.readCommitted();
        assertEquals(endOfBlock(data, 0), r.position());
    }

    @Test
    void readCommittedReturnsNullAtEof() throws Exception {
        byte[] data = build("a".getBytes());
        WalCursorReader r = new WalCursorReader(data, data.length);
        assertArrayEquals("a".getBytes(), r.readCommitted());
        assertNull(r.readCommitted());
    }

    @Test
    void dirtyReturnsNullAtEof() throws Exception {
        byte[] data = build("a".getBytes());
        WalCursorReader r = new WalCursorReader(data);
        assertArrayEquals("a".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void readCommittedTruncatedHeaderTypedError() throws Exception {
        // 2 bytes - shorter than a 4-byte header.
        byte[] data = new byte[]{0, 0};
        WalCursorReader r = new WalCursorReader(data, 2);
        assertThrows(SegmentReader.TruncatedFrame.class, r::readCommitted);
    }

    @Test
    void dirtyTruncatedSurfacesTypedError() throws Exception {
        byte[] hand = new byte[]{0, 0, 0, 10, 'a', 'b', 'c'};
        WalCursorReader r = new WalCursorReader(hand);
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void negativeFrameLengthRejected() throws Exception {
        byte[] hand = new byte[]{(byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff};
        WalCursorReader r = new WalCursorReader(hand, hand.length);
        assertThrows(java.io.IOException.class, r::readCommitted);
    }
}
