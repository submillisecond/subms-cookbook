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

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Sample app: a tour of {@code subms-segment-reader} against an order-event
 * journal, base API first, then each optional reader. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.segment.SampleApp}
 *
 * <ul>
 *   <li>base        - replay an order journal, stopping cleanly on a crash-torn tail
 *   <li>mmap        - map a large capture file and page it in lazily
 *   <li>crc32       - detect a bit-flip in an at-rest capture (RocksDB-style trailer)
 *   <li>xxh3        - faster integrity check for a trusted internal pipeline
 *   <li>lz4         - compressed capture blocks that decompress on read
 *   <li>seek-index  - jump to the millionth event without scanning from the head
 *   <li>wal-cursor  - replay only what the writer has fsync'd, never a partial tail
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws IOException {
        baseJournalReplay();
        mmapCaptureReplay();
        crc32CorruptionGuard();
        xxh3FastIntegrity();
        lz4CompressedBlocks();
        seekToEvent();
        durableReplay();
    }

    /** Encode a run of order events into a framed segment. */
    static byte[] buildJournal(String... events) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (String e : events) w.write(e.getBytes(StandardCharsets.UTF_8));
        dos.close();
        return baos.toByteArray();
    }

    /** Base API: a matching engine replays its order journal on restart.
     *  The process died mid-append last time, so the segment ends torn. */
    static void baseJournalReplay() throws IOException {
        System.out.println("== base: order-journal replay after a crash ==");
        byte[] intact = buildJournal(
            "NEW  AAPL  buy  100 @ 150.25",
            "NEW  MSFT  sell  50 @ 402.10",
            "CXL  AAPL  ord-1");
        // The writer was cut mid-frame: a header claiming 32 bytes, 4 written.
        byte[] segment = new byte[intact.length + 8];
        System.arraycopy(intact, 0, segment, 0, intact.length);
        segment[intact.length + 3] = 32;
        segment[intact.length + 4] = 'N';
        segment[intact.length + 5] = 'E';
        segment[intact.length + 6] = 'W';
        segment[intact.length + 7] = ' ';

        SegmentReader r = new SegmentReader(new DataInputStream(new ByteArrayInputStream(segment)));
        int replayed = 0;
        while (true) {
            try {
                byte[] event = r.nextRecord();
                if (event == null) {
                    System.out.println("  -> clean EOF");
                    break;
                }
                System.out.println("  replay " + new String(event, StandardCharsets.UTF_8));
                replayed++;
            } catch (SegmentReader.TruncatedFrame torn) {
                System.out.println("  -> stopped at torn tail: " + torn.getMessage());
                break;
            }
        }
        System.out.println("  replayed " + replayed + " intact events");
        if (replayed != 3) throw new AssertionError("every event before the torn tail is recovered");
    }

    /** mmap: map a capture file in constant time; the OS pages in only the
     *  frames actually touched. */
    static void mmapCaptureReplay() throws IOException {
        System.out.println("\n== mmap: replay a capture file without loading it whole ==");
        byte[] segment = buildJournal("TICK AAPL 150.25", "TICK AAPL 150.26", "TICK MSFT 402.11");
        Path path = Files.createTempFile("subms-segment-sample-", ".capture");
        Files.write(path, segment);

        int ticks = 0;
        try (MmapSegmentReader r = new MmapSegmentReader(path)) {
            System.out.println("  mapped " + r.length() + " bytes, resident set grows on touch");
            byte[] event;
            while ((event = r.nextRecord()) != null) {
                System.out.println("  " + new String(event, StandardCharsets.UTF_8));
                ticks++;
            }
        } finally {
            Files.deleteIfExists(path);
        }
        if (ticks != 3) throw new AssertionError("mmap path reads the same frames as the base reader");
    }

    /** crc32: a CRC32C trailer per block turns silent corruption into a
     *  typed ChecksumMismatch on read. */
    static void crc32CorruptionGuard() throws IOException {
        System.out.println("\n== crc32: catch a bit-flip in an at-rest capture ==");
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Crc32SegmentWriter w = new Crc32SegmentWriter(dos);
        w.write("FILL AAPL 100 @ 150.25".getBytes(StandardCharsets.UTF_8));
        w.write("FILL MSFT  50 @ 402.10".getBytes(StandardCharsets.UTF_8));
        dos.close();
        byte[] segment = baos.toByteArray();
        segment[4] ^= 0x08; // flip a bit in the first payload

        Crc32SegmentReader r = new Crc32SegmentReader(
            new DataInputStream(new ByteArrayInputStream(segment)));
        try {
            r.nextRecord();
            throw new AssertionError("expected ChecksumMismatch on corrupted block");
        } catch (Crc32SegmentReader.ChecksumMismatch expected) {
            System.out.println("  block 0: ChecksumMismatch (corruption caught)");
        }
    }

    /** xxh3: a cheaper integrity check for a trusted, single-implementation
     *  pipeline. Catches accidental corruption; not adversary-safe. */
    static void xxh3FastIntegrity() throws IOException {
        System.out.println("\n== xxh3: faster per-block integrity on a trusted pipeline ==");
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        Xxh3SegmentWriter w = new Xxh3SegmentWriter(dos);
        for (String e : new String[] {"FILL AAPL 100 @ 150.25", "FILL AAPL  25 @ 150.26"}) {
            w.write(e.getBytes(StandardCharsets.UTF_8));
        }
        dos.close();

        Xxh3SegmentReader r = new Xxh3SegmentReader(
            new DataInputStream(new ByteArrayInputStream(baos.toByteArray())));
        int ok = 0;
        byte[] event;
        while ((event = r.nextRecord()) != null) {
            System.out.println("  verified " + new String(event, StandardCharsets.UTF_8));
            ok++;
        }
        if (ok != 2) throw new AssertionError("both blocks pass their xxh3 trailer");
    }

    /** lz4: repetitive order events compress well; the reader decompresses
     *  transparently on read. */
    static void lz4CompressedBlocks() throws IOException {
        System.out.println("\n== lz4: compressed capture blocks ==");
        byte[] payload = "NEW AAPL buy 100 @ 150.25\n".repeat(256).getBytes(StandardCharsets.UTF_8);
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new Lz4BlockWriter(dos).write(payload);
        dos.close();
        byte[] segment = baos.toByteArray();
        System.out.println("  " + payload.length + " raw bytes stored in " + segment.length + " on-disk bytes");
        if (segment.length >= payload.length) throw new AssertionError("repetitive block should compress");

        Lz4SegmentReader r = new Lz4SegmentReader(
            new DataInputStream(new ByteArrayInputStream(segment)));
        byte[] block = r.nextRecord();
        if (!java.util.Arrays.equals(block, payload)) {
            throw new AssertionError("decompresses byte-for-byte");
        }
        System.out.println("  decompressed back to " + block.length + " bytes");
    }

    /** seek-index: index every 64th block at open, then binary-search plus a
     *  bounded scan to reach any block. */
    static void seekToEvent() throws IOException {
        System.out.println("\n== seek-index: jump to event 100 without scanning the head ==");
        String[] events = new String[200];
        for (int i = 0; i < events.length; i++) events[i] = "SEQ " + i + ": TICK AAPL";
        byte[] segment = buildJournal(events);

        IndexedSegmentReader r = new IndexedSegmentReader(segment);
        System.out.println("  " + r.totalBlocks() + " blocks, " + r.indexLen() + " sparse index entries");
        r.seekToBlock(100);
        byte[] at = r.nextRecord();
        System.out.println("  seek(100) -> " + new String(at, StandardCharsets.UTF_8));
        if (!java.util.Arrays.equals(at, "SEQ 100: TICK AAPL".getBytes(StandardCharsets.UTF_8))) {
            throw new AssertionError("landed on the requested event");
        }
    }

    /** wal-cursor: readCommitted() stops at the durable watermark; advancing
     *  it exposes the newly-committed prefix. */
    static void durableReplay() throws IOException {
        System.out.println("\n== wal-cursor: replay only the fsync'd prefix ==");
        byte[] segment = buildJournal("FILL ord-1", "FILL ord-2", "FILL ord-3");
        int afterFirst = 4 + "FILL ord-1".length();

        WalCursorReader r = new WalCursorReader(segment);
        if (r.readCommitted() != null) throw new AssertionError("watermark at 0 -> nothing durable");

        r.setCommitted(afterFirst);
        byte[] event = r.readCommitted();
        System.out.println("  after first fsync -> " + new String(event, StandardCharsets.UTF_8));
        if (r.readCommitted() != null) throw new AssertionError("second block not yet committed");

        r.setCommitted(segment.length);
        int tail = 0;
        while ((event = r.readCommitted()) != null) {
            System.out.println("  now durable -> " + new String(event, StandardCharsets.UTF_8));
            tail++;
        }
        if (tail != 2) throw new AssertionError("the remaining committed events replay");
    }

    private SampleApp() {}
}
