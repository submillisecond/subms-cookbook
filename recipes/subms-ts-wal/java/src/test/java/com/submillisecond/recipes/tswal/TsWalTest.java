package com.submillisecond.recipes.tswal;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Comparator;
import java.util.List;
import java.util.stream.Stream;

import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

class TsWalTest {

    private Path dir;

    @BeforeEach
    void setUp() throws IOException {
        dir = Files.createTempDirectory("subms-ts-wal-test-");
    }

    @AfterEach
    void tearDown() throws IOException {
        if (dir != null && Files.exists(dir)) {
            try (Stream<Path> walk = Files.walk(dir)) {
                walk.sorted(Comparator.reverseOrder()).forEach(p -> {
                    try {
                        Files.deleteIfExists(p);
                    } catch (IOException ignored) {
                        // best-effort
                    }
                });
            }
        }
    }

    private List<String> listSegments() throws IOException {
        try (Stream<Path> s = Files.list(dir)) {
            return s.map(p -> p.getFileName().toString())
                    .filter(n -> n.startsWith("wal-") && n.endsWith(".log"))
                    .sorted()
                    .toList();
        }
    }

    @Test
    void appendReplayRoundTripBitExact() {
        double[] vals = {1.5, -2.25, 0.0, Double.MAX_VALUE, Double.MIN_VALUE, 12345.678901};
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            for (int i = 0; i < vals.length; i++) {
                wal.append(42, i, vals[i]);
            }
            wal.flush();
        }
        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(vals.length, recs.size());
        for (int i = 0; i < vals.length; i++) {
            assertEquals(42, recs.get(i).seriesId());
            assertEquals(i, recs.get(i).ts());
            assertEquals(Double.doubleToLongBits(vals[i]),
                    Double.doubleToLongBits(recs.get(i).value()), "bit-exact at " + i);
        }
    }

    @Test
    void nanValueRoundTripsBitExact() {
        double nan = Double.longBitsToDouble(0x7ff8_0000_0000_0001L);
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 1, nan);
            wal.flush();
        }
        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(1, recs.size());
        assertTrue(Double.isNaN(recs.get(0).value()));
        assertEquals(Double.doubleToLongBits(nan), Double.doubleToLongBits(recs.get(0).value()));
    }

    @Test
    void reopenPreservesBothSessionsInOrder() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.ALWAYS)) {
            wal.append(1, 10, 1.0);
            wal.append(1, 11, 2.0);
        }
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.ALWAYS)) {
            wal.append(1, 12, 3.0);
            wal.append(1, 13, 4.0);
            wal.flush();
        }
        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(List.of(10L, 11L, 12L, 13L), recs.stream().map(TsWalRecord::ts).toList());
        assertEquals(List.of(1.0, 2.0, 3.0, 4.0), recs.stream().map(TsWalRecord::value).toList());
    }

    @Test
    void reopenStartsFreshSegmentAfterHighestSeq() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 1, 1.0);
            wal.flush();
            assertEquals(0, wal.activeSeq());
        }
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            assertEquals(1, wal.activeSeq());
        }
    }

    @Test
    void segmentRolloverProducesMultipleFilesAndReplaysAll() throws IOException {
        long total = TsWal.SEGMENT_MAX_RECORDS * 2 + 100;
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            for (long i = 0; i < total; i++) {
                wal.append(9, i, (double) i);
            }
            wal.flush();
        }
        List<String> segs = listSegments();
        assertTrue(segs.size() >= 3, "expected >=3 segments, got " + segs);
        assertEquals("wal-0000000000.log", segs.get(0));

        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(total, recs.size());
        for (int i = 0; i < total; i++) {
            assertEquals(i, recs.get(i).ts());
        }
    }

    @Test
    void truncationSafetyGarbageTailReturnsValidPrefix() throws IOException {
        long good = 50;
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            for (long i = 0; i < good; i++) {
                wal.append(3, i, (double) i);
            }
            wal.flush();
        }
        // Simulate a crash mid-append: 10 stray bytes appended to the active file.
        Path active = dir.resolve("wal-0000000000.log");
        Files.write(active, new byte[]{(byte) 0xAB, (byte) 0xAB, (byte) 0xAB, (byte) 0xAB,
                (byte) 0xAB, (byte) 0xAB, (byte) 0xAB, (byte) 0xAB, (byte) 0xAB, (byte) 0xAB},
                StandardOpenOption.APPEND);

        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(good, recs.size(), "torn tail must not poison recovery");
        for (int i = 0; i < good; i++) {
            assertEquals(i, recs.get(i).ts());
        }
    }

    @Test
    void truncationSafetyHalfRecordDropped() throws IOException {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 0, 0.0);
            wal.append(1, 1, 1.0);
            wal.flush();
        }
        // A full record minus its last 4 bytes: a torn write of a third record.
        byte[] partial = TsWal.encodeRecord(1, 2, 2.0);
        byte[] torn = new byte[TsWal.RECORD_LEN - 4];
        System.arraycopy(partial, 0, torn, 0, torn.length);
        Files.write(dir.resolve("wal-0000000000.log"), torn, StandardOpenOption.APPEND);

        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(2, recs.size());
        assertEquals(1, recs.get(1).ts());
    }

    @Test
    void crcCorruptionStopsAtBadRecord() throws IOException {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            for (long i = 0; i < 5; i++) {
                wal.append(1, i, (double) i);
            }
            wal.flush();
        }
        // Flip a byte inside record 2's value field; CRC no longer matches.
        Path active = dir.resolve("wal-0000000000.log");
        byte[] bytes = Files.readAllBytes(active);
        int target = 2 * TsWal.RECORD_LEN + 16;
        bytes[target] ^= 0xFF;
        Files.write(active, bytes);

        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(2, recs.size(), "replay stops at the corrupt record");
        assertEquals(0, recs.get(0).ts());
        assertEquals(1, recs.get(1).ts());
    }

    @Test
    void truncateBeforeDropsOnlyOldSealedSegments() throws IOException {
        long per = TsWal.SEGMENT_MAX_RECORDS;
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            for (long i = 0; i < per * 2 + 5; i++) {
                wal.append(1, i, (double) i);
            }
            wal.flush();
            assertTrue(listSegments().size() >= 3);

            long cutoff = per + 10;
            int removed = wal.truncateBefore(cutoff);
            assertEquals(1, removed, "only segment 0 should be removed");
            assertFalse(Files.exists(dir.resolve("wal-0000000000.log")));
            assertTrue(Files.exists(dir.resolve("wal-0000000001.log")));

            List<TsWalRecord> recs = wal.replay();
            assertEquals(per, recs.get(0).ts());
            assertEquals(per * 2 + 4, recs.get(recs.size() - 1).ts());
        }
    }

    @Test
    void truncateBeforeNeverTouchesActiveSegment() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 1, 1.0);
            wal.append(1, 2, 2.0);
            wal.flush();
            int removed = wal.truncateBefore(1_000_000);
            assertEquals(0, removed);
            assertEquals(2, wal.replay().size());
        }
    }

    @Test
    void policyEveryNAppendsCorrectness() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.everyNAppends(8))) {
            for (long i = 0; i < 20; i++) {
                wal.append(1, i, (double) i);
            }
            wal.flush();
        }
        assertEquals(20, TsWal.open(dir, TsFsyncPolicy.NEVER).replay().size());
    }

    @Test
    void policyEveryNMillisCorrectness() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.everyNMillis(1))) {
            for (long i = 0; i < 30; i++) {
                wal.append(1, i, (double) i);
            }
            wal.flush();
        }
        assertEquals(30, TsWal.open(dir, TsFsyncPolicy.NEVER).replay().size());
    }

    @Test
    void policyAlwaysCorrectness() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.ALWAYS)) {
            for (long i = 0; i < 15; i++) {
                wal.append(1, i, (double) i);
            }
        }
        assertEquals(15, TsWal.open(dir, TsFsyncPolicy.NEVER).replay().size());
    }

    @Test
    void emptyWalReplaysEmpty() {
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            assertTrue(wal.replay().isEmpty());
        }
    }

    @Test
    void freshOpenCreatesDirIfAbsent() {
        Path nested = dir.resolve("a").resolve("b");
        try (TsWal wal = TsWal.open(nested, TsFsyncPolicy.NEVER)) {
            assertTrue(Files.isDirectory(nested));
            assertTrue(wal.replay().isEmpty());
        }
    }

    // Pins the cross-language record layout: seriesId=7, ts=100, value=1.5.
    // Identical to the Rust RECORD_FIXTURE constant, proving the LE field order
    // and CRC-32 (java.util.zip.CRC32) match the Rust hand-roll bit-for-bit.
    private static final String RECORD_FIXTURE =
            "07000000000000006400000000000000000000000000f83fdf7aaa15";

    private static String toHex(byte[] b) {
        StringBuilder sb = new StringBuilder(b.length * 2);
        for (byte x : b) {
            sb.append(String.format("%02x", x & 0xFF));
        }
        return sb.toString();
    }

    @Test
    void closeForcesDirtyBufferWithoutExplicitFlush() {
        // NEVER policy + no flush: close() must hit the dirty-force path so the
        // last appends are durable, and a reopen replays them.
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 1, 1.0);
            wal.append(1, 2, 2.0);
        }
        assertEquals(2, TsWal.open(dir, TsFsyncPolicy.NEVER).replay().size());
    }

    @Test
    void scanIgnoresNonSegmentFiles() throws IOException {
        // Exercises every parseSegmentSeq rejection branch: wrong prefix, wrong
        // length, and a non-digit character in the seq slot.
        try (TsWal wal = TsWal.open(dir, TsFsyncPolicy.NEVER)) {
            wal.append(1, 1, 1.0);
            wal.flush();
        }
        Files.writeString(dir.resolve("README.txt"), "not a segment");
        Files.writeString(dir.resolve("wal-1.log"), "wrong length");
        Files.writeString(dir.resolve("wal-00000000xx.log"), "non digit");
        Files.writeString(dir.resolve("wal-0000000000.dat"), "wrong suffix");

        List<TsWalRecord> recs = TsWal.open(dir, TsFsyncPolicy.NEVER).replay();
        assertEquals(1, recs.size());
        assertEquals(1, recs.get(0).ts());
    }

    @Test
    void exceptionCarriesKindAndCause() {
        IOException cause = new IOException("disk gone");
        TsWalException withCause = new TsWalException(TsWalException.Kind.IO, "boom", cause);
        assertEquals(TsWalException.Kind.IO, withCause.kind());
        assertEquals(cause, withCause.getCause());
        assertEquals("boom", withCause.getMessage());

        TsWalException corrupt = new TsWalException(TsWalException.Kind.CORRUPT, "bad dir");
        assertEquals(TsWalException.Kind.CORRUPT, corrupt.kind());
        // values() reachability for the enum.
        assertEquals(2, TsWalException.Kind.values().length);
    }

    @Test
    void recordEncodingMatchesFixture() {
        assertEquals(RECORD_FIXTURE, toHex(TsWal.encodeRecord(7, 100, 1.5)));
    }

    @Test
    void encodeRecordIsStable() {
        assertArrayEquals(TsWal.encodeRecord(7, 100, 1.5), TsWal.encodeRecord(7, 100, 1.5));
    }
}
