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

final class IndexedSegmentReaderTest {

    private static byte[] buildN(int n) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (int i = 0; i < n; i++) {
            w.write(("rec-" + i).getBytes());
        }
        dos.close();
        return baos.toByteArray();
    }

    @Test
    void indexStrideMatchesBlockLayout() throws Exception {
        byte[] data = buildN(200);
        IndexedSegmentReader r = new IndexedSegmentReader(data);
        assertEquals(200, r.totalBlocks());
        // 200 blocks at stride 64 -> entries at 0, 64, 128, 192 = 4 entries.
        assertEquals(4, r.indexLen());
    }

    @Test
    void seekToFirstBlock() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(200));
        r.seekToBlock(0);
        assertArrayEquals("rec-0".getBytes(), r.nextRecord());
    }

    @Test
    void seekToIndexedBlock() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(200));
        r.seekToBlock(128);
        assertArrayEquals("rec-128".getBytes(), r.nextRecord());
    }

    @Test
    void seekToUnindexedBlockScansForward() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(200));
        // 100 sits between index entries 64 and 128.
        r.seekToBlock(100);
        assertArrayEquals("rec-100".getBytes(), r.nextRecord());
    }

    @Test
    void seekPastEndYieldsNull() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(50));
        r.seekToBlock(9999);
        assertNull(r.nextRecord());
    }

    @Test
    void openOnCorruptedTailErrors() throws Exception {
        byte[] data = buildN(10);
        byte[] dirty = new byte[data.length + 3]; // 3 extra bytes - truncated header
        System.arraycopy(data, 0, dirty, 0, data.length);
        assertThrows(SegmentReader.TruncatedFrame.class, () -> new IndexedSegmentReader(dirty));
    }

    @Test
    void sequentialNextRecordWalksEveryBlock() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(40));
        for (int i = 0; i < 40; i++) {
            assertArrayEquals(("rec-" + i).getBytes(), r.nextRecord());
        }
        assertNull(r.nextRecord());
    }

    @Test
    void emptySegmentIndexIsEmpty() throws Exception {
        IndexedSegmentReader r = new IndexedSegmentReader(new byte[0]);
        assertEquals(0, r.totalBlocks());
        assertEquals(0, r.indexLen());
    }

    @Test
    void negativeFrameLengthRejected() {
        byte[] hand = new byte[]{(byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff};
        assertThrows(java.io.IOException.class, () -> new IndexedSegmentReader(hand));
    }

    @Test
    void seekToLastIndexedBlockOnSmallSegment() throws Exception {
        // <= INDEX_STRIDE blocks: only one index entry at block 0.
        IndexedSegmentReader r = new IndexedSegmentReader(buildN(10));
        assertEquals(1, r.indexLen());
        r.seekToBlock(5);
        assertArrayEquals("rec-5".getBytes(), r.nextRecord());
    }
}
