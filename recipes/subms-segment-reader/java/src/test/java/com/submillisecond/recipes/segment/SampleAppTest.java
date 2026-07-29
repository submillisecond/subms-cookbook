package com.submillisecond.recipes.segment;

import com.submillisecond.recipes.segment.features.Crc32SegmentReader;
import com.submillisecond.recipes.segment.features.Crc32SegmentWriter;
import com.submillisecond.recipes.segment.features.IndexedSegmentReader;
import com.submillisecond.recipes.segment.features.Lz4BlockWriter;
import com.submillisecond.recipes.segment.features.Lz4SegmentReader;
import com.submillisecond.recipes.segment.features.MmapSegmentReader;
import com.submillisecond.recipes.segment.features.WalCursorReader;
import com.submillisecond.recipes.segment.features.Xxh3SegmentReader;
import com.submillisecond.recipes.segment.features.Xxh3SegmentWriter;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() throws Exception {
        // quickstart:begin
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        SegmentWriter w = new SegmentWriter(new DataOutputStream(bytes));
        w.write("alice".getBytes());
        w.write("bob".getBytes());

        DataInputStream in = new DataInputStream(new ByteArrayInputStream(bytes.toByteArray()));
        SegmentReader r = new SegmentReader(in);
        assertArrayEquals("alice".getBytes(), r.nextRecord());
        assertArrayEquals("bob".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());   // clean EOF
        // quickstart:end
    }

    @Test
    void journalReplayStopsOnTornTail() throws Exception {
        byte[] intact = SampleApp.buildJournal(
            "NEW  AAPL  buy  100 @ 150.25",
            "NEW  MSFT  sell  50 @ 402.10",
            "CXL  AAPL  ord-1");
        byte[] segment = new byte[intact.length + 8];
        System.arraycopy(intact, 0, segment, 0, intact.length);
        segment[intact.length + 3] = 32;
        segment[intact.length + 4] = 'N';
        segment[intact.length + 5] = 'E';
        segment[intact.length + 6] = 'W';
        segment[intact.length + 7] = ' ';

        SegmentReader r = new SegmentReader(new DataInputStream(new ByteArrayInputStream(segment)));
        assertArrayEquals("NEW  AAPL  buy  100 @ 150.25".getBytes(), r.nextRecord());
        assertArrayEquals("NEW  MSFT  sell  50 @ 402.10".getBytes(), r.nextRecord());
        assertArrayEquals("CXL  AAPL  ord-1".getBytes(), r.nextRecord());
        assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
    }

    @Test
    void mmapReadsSameFrames() throws Exception {
        byte[] segment = SampleApp.buildJournal("TICK AAPL 150.25", "TICK MSFT 402.11");
        Path path = Files.createTempFile("subms-segment-sample-test-", ".capture");
        Files.write(path, segment);
        try (MmapSegmentReader r = new MmapSegmentReader(path)) {
            assertArrayEquals("TICK AAPL 150.25".getBytes(), r.nextRecord());
            assertArrayEquals("TICK MSFT 402.11".getBytes(), r.nextRecord());
            assertNull(r.nextRecord());
        } finally {
            Files.deleteIfExists(path);
        }
    }

    @Test
    void crc32CatchesBitFlip() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Crc32SegmentWriter(dos).write("FILL AAPL 100 @ 150.25".getBytes());
        dos.close();
        byte[] segment = baos.toByteArray();
        segment[4] ^= 0x08;
        Crc32SegmentReader r = new Crc32SegmentReader(
            new DataInputStream(new ByteArrayInputStream(segment)));
        assertThrows(Crc32SegmentReader.ChecksumMismatch.class, r::nextRecord);
    }

    @Test
    void xxh3RoundTripsCleanBlocks() throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Xxh3SegmentWriter w = new Xxh3SegmentWriter(dos);
        w.write("FILL AAPL 100 @ 150.25".getBytes());
        w.write("FILL AAPL  25 @ 150.26".getBytes());
        dos.close();
        Xxh3SegmentReader r = new Xxh3SegmentReader(
            new DataInputStream(new ByteArrayInputStream(baos.toByteArray())));
        assertArrayEquals("FILL AAPL 100 @ 150.25".getBytes(), r.nextRecord());
        assertArrayEquals("FILL AAPL  25 @ 150.26".getBytes(), r.nextRecord());
        assertNull(r.nextRecord());
    }

    @Test
    void lz4CompressesAndRoundTrips() throws Exception {
        byte[] payload = "NEW AAPL buy 100 @ 150.25\n".repeat(256).getBytes(StandardCharsets.UTF_8);
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Lz4BlockWriter(dos).write(payload);
        dos.close();
        byte[] segment = baos.toByteArray();
        assertTrue(segment.length < payload.length, "repetitive block compressed");
        Lz4SegmentReader r = new Lz4SegmentReader(
            new DataInputStream(new ByteArrayInputStream(segment)));
        assertArrayEquals(payload, r.nextRecord());
    }

    @Test
    void seekIndexLandsOnRequestedEvent() throws Exception {
        String[] events = new String[200];
        for (int i = 0; i < events.length; i++) events[i] = "SEQ " + i + ": TICK AAPL";
        byte[] segment = SampleApp.buildJournal(events);
        IndexedSegmentReader r = new IndexedSegmentReader(segment);
        assertEquals(200, r.totalBlocks());
        r.seekToBlock(100);
        assertArrayEquals("SEQ 100: TICK AAPL".getBytes(), r.nextRecord());
    }

    @Test
    void walCursorGatesOnWatermark() throws Exception {
        byte[] segment = SampleApp.buildJournal("FILL ord-1", "FILL ord-2", "FILL ord-3");
        int afterFirst = 4 + "FILL ord-1".length();
        WalCursorReader r = new WalCursorReader(segment);
        assertNull(r.readCommitted(), "nothing durable yet");
        r.setCommitted(afterFirst);
        assertArrayEquals("FILL ord-1".getBytes(), r.readCommitted());
        assertNull(r.readCommitted(), "second block not committed");
        r.setCommitted(segment.length);
        assertArrayEquals("FILL ord-2".getBytes(), r.readCommitted());
        assertArrayEquals("FILL ord-3".getBytes(), r.readCommitted());
        assertNull(r.readCommitted());
    }
}
