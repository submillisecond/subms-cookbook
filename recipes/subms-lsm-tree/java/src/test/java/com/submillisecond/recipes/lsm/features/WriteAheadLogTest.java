package com.submillisecond.recipes.lsm.features;

import com.submillisecond.recipes.lsm.features.WriteAheadLog.WalEntry;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class WriteAheadLogTest {

    @Test
    void replayIsEmptyForMissingFile(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("does-not-exist.log");
        assertTrue(WriteAheadLog.replay(p).isEmpty());
    }

    @Test
    void roundTripPutAndDelete(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("a", "alpha".getBytes());
            wal.logPut("b", "beta".getBytes());
            wal.logDelete("a");
            wal.sync();
        }
        List<WalEntry> entries = WriteAheadLog.replay(p);
        assertEquals(3, entries.size());
        assertEquals("a", entries.get(0).key());
        assertArrayEquals("alpha".getBytes(), entries.get(0).value());
        assertEquals("b", entries.get(1).key());
        assertArrayEquals("beta".getBytes(), entries.get(1).value());
        assertEquals("a", entries.get(2).key());
        assertNull(entries.get(2).value(), "delete must replay as tombstone");
    }

    @Test
    void truncateDropsPriorRecords(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("x", "y".getBytes());
            wal.truncate();
        }
        assertTrue(WriteAheadLog.replay(p).isEmpty(), "post-truncate replay must be empty");
    }

    @Test
    void replayRecoversAcrossReopen(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("k1", "v1".getBytes());
            wal.sync();
        }
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("k2", "v2".getBytes());
            wal.sync();
        }
        List<WalEntry> entries = WriteAheadLog.replay(p);
        assertEquals(2, entries.size());
        assertEquals("k1", entries.get(0).key());
        assertEquals("k2", entries.get(1).key());
    }

    @Test
    void tornTailIsDroppedWithoutCorruptingPrefix(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("good", "value".getBytes());
            wal.sync();
        }
        // Append junk past the last valid record: simulates a half-written tail.
        Files.write(p, new byte[]{0x00, 0x00, 0x00, 0x00, 0x05},
                StandardOpenOption.WRITE, StandardOpenOption.APPEND);
        List<WalEntry> entries = WriteAheadLog.replay(p);
        assertEquals(1, entries.size(), "torn tail must not be replayed");
        assertEquals("good", entries.get(0).key());
    }

    @Test
    void crcCorruptionTruncatesAtBadRecord(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("first", "ok".getBytes());
            wal.logPut("second", "corrupted".getBytes());
            wal.sync();
        }
        byte[] buf = Files.readAllBytes(p);
        // Flip a byte inside the SECOND record's payload (~14 from end).
        buf[buf.length - 8] ^= (byte) 0xff;
        Files.write(p, buf, StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING);
        List<WalEntry> entries = WriteAheadLog.replay(p);
        assertEquals(1, entries.size(), "corrupt record drops everything after");
        assertEquals("first", entries.get(0).key());
    }

    @Test
    void emptyValuePutRoundTrips(@TempDir Path dir) throws IOException {
        Path p = dir.resolve("wal.log");
        try (WriteAheadLog wal = new WriteAheadLog(p)) {
            wal.logPut("k", new byte[0]);
            wal.sync();
        }
        List<WalEntry> entries = WriteAheadLog.replay(p);
        assertEquals(1, entries.size());
        assertArrayEquals(new byte[0], entries.get(0).value());
    }
}
