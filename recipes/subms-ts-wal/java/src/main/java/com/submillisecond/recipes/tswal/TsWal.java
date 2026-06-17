package com.submillisecond.recipes.tswal;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.zip.CRC32;
import java.util.stream.Stream;

/**
 * Append-only write-ahead log for f64-valued time-series records, with
 * truncation-safe crash recovery.
 *
 * <p>The log is a directory of fixed-width segment files named
 * {@code wal-<10-digit-seq>.log}. Each record is 28 bytes, little-endian, with a
 * trailing CRC-32 (IEEE 0xEDB88320, via {@link CRC32}) over its 24-byte payload:
 *
 * <pre>
 *   [seriesId u64 LE][ts i64 LE][valueBits u64 LE][crc32 u32 LE]
 * </pre>
 *
 * <p>{@code valueBits} is {@link Double#doubleToLongBits}, so the value
 * round-trips bit-exact. Segments roll to a fresh file every
 * {@link #SEGMENT_MAX_RECORDS}. {@link #replay()} CRC-validates every record and
 * stops at the first short, garbage, or checksum-failing record, returning the
 * valid prefix - a crash mid-append loses only the uncommitted tail.
 *
 * <p>Byte-equivalent to the Rust {@code subms-ts-wal} crate: the record layout
 * and the CRC match bit-for-bit (a hex fixture pins it).
 *
 * <p>This is a server-side durability primitive. It uses {@link FileChannel}
 * and {@code force} for the fsync; it is not usable in environments without a
 * filesystem.
 */
public final class TsWal implements AutoCloseable {

    /** Records per segment before the log rolls to a fresh file. */
    public static final long SEGMENT_MAX_RECORDS = 4096L;

    /** Wire size of one record: 8 + 8 + 8 + 4. */
    public static final int RECORD_LEN = 28;

    /** Payload bytes the CRC is taken over. */
    private static final int PAYLOAD_LEN = 24;

    private final Path dir;
    private final TsFsyncPolicy policy;

    private long activeSeq;
    private FileChannel channel;
    private long recordsInSegment;
    private int appendsSinceSync;
    private long lastSyncNanos;
    private boolean dirty;

    private TsWal(Path dir, TsFsyncPolicy policy) {
        this.dir = dir;
        this.policy = policy;
    }

    /**
     * Open (creating if absent) the log at {@code dir}. Scans existing
     * {@code wal-*.log} segments and starts a fresh active segment after the
     * highest existing sequence, so a reopen preserves all prior data for
     * {@link #replay()}.
     */
    public static TsWal open(Path dir, TsFsyncPolicy policy) {
        TsWal wal = new TsWal(dir, policy);
        try {
            Files.createDirectories(dir);
            List<Long> seqs = scanSegments(dir);
            long highest = seqs.isEmpty() ? -1L : seqs.get(seqs.size() - 1);
            wal.activeSeq = highest + 1;
            wal.channel = openSegment(dir, wal.activeSeq);
            wal.recordsInSegment = 0;
            wal.appendsSinceSync = 0;
            wal.lastSyncNanos = System.nanoTime();
            wal.dirty = false;
            return wal;
        } catch (IOException e) {
            throw new TsWalException(TsWalException.Kind.IO, "open failed: " + dir, e);
        }
    }

    /** All segment sequence numbers present in {@code dir}, ascending. */
    private static List<Long> scanSegments(Path dir) throws IOException {
        List<Long> seqs = new ArrayList<>();
        try (Stream<Path> stream = Files.list(dir)) {
            stream.forEach(p -> {
                Long seq = parseSegmentSeq(p.getFileName().toString());
                if (seq != null) {
                    seqs.add(seq);
                }
            });
        }
        seqs.sort(Long::compareTo);
        return seqs;
    }

    private static Long parseSegmentSeq(String name) {
        if (!name.startsWith("wal-") || !name.endsWith(".log")) {
            return null;
        }
        String rest = name.substring(4, name.length() - 4);
        if (rest.length() != 10) {
            return null;
        }
        for (int i = 0; i < rest.length(); i++) {
            if (rest.charAt(i) < '0' || rest.charAt(i) > '9') {
                return null;
            }
        }
        try {
            return Long.parseLong(rest);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static String segmentName(long seq) {
        return String.format("wal-%010d.log", seq);
    }

    private static FileChannel openSegment(Path dir, long seq) throws IOException {
        return FileChannel.open(
                dir.resolve(segmentName(seq)),
                StandardOpenOption.CREATE,
                StandardOpenOption.WRITE,
                StandardOpenOption.APPEND);
    }

    /** Encode a record into its 28-byte on-disk form. */
    public static byte[] encodeRecord(long seriesId, long ts, double value) {
        ByteBuffer buf = ByteBuffer.allocate(RECORD_LEN).order(ByteOrder.LITTLE_ENDIAN);
        buf.putLong(seriesId);
        buf.putLong(ts);
        buf.putLong(Double.doubleToLongBits(value));
        CRC32 crc = new CRC32();
        crc.update(buf.array(), 0, PAYLOAD_LEN);
        buf.putInt((int) crc.getValue());
        return buf.array();
    }

    /**
     * Decode + CRC-validate one record from {@code bytes} at {@code offset}.
     * Returns {@code null} if short or the checksum does not match - both signal
     * a torn or corrupt tail.
     */
    private static TsWalRecord decodeRecord(byte[] bytes, int offset) {
        if (offset + RECORD_LEN > bytes.length) {
            return null;
        }
        CRC32 crc = new CRC32();
        crc.update(bytes, offset, PAYLOAD_LEN);
        ByteBuffer buf = ByteBuffer.wrap(bytes, offset, RECORD_LEN).order(ByteOrder.LITTLE_ENDIAN);
        long seriesId = buf.getLong();
        long ts = buf.getLong();
        long valueBits = buf.getLong();
        int stored = buf.getInt();
        if ((int) crc.getValue() != stored) {
            return null;
        }
        return new TsWalRecord(seriesId, ts, Double.longBitsToDouble(valueBits));
    }

    /**
     * Append one record. Rolls to a new segment when the active one is full,
     * then applies the fsync policy.
     */
    public void append(long seriesId, long ts, double value) {
        try {
            if (recordsInSegment >= SEGMENT_MAX_RECORDS) {
                rollSegment();
            }
            ByteBuffer buf = ByteBuffer.wrap(encodeRecord(seriesId, ts, value));
            while (buf.hasRemaining()) {
                channel.write(buf);
            }
            recordsInSegment++;
            appendsSinceSync++;
            dirty = true;
            maybeSync();
        } catch (IOException e) {
            throw new TsWalException(TsWalException.Kind.IO, "append failed", e);
        }
    }

    private void rollSegment() throws IOException {
        syncNow();
        channel.close();
        activeSeq++;
        channel = openSegment(dir, activeSeq);
        recordsInSegment = 0;
    }

    private void maybeSync() throws IOException {
        boolean should = switch (policy.kind()) {
            case ALWAYS -> true;
            case EVERY_N_APPENDS -> policy.arg() != 0 && appendsSinceSync >= policy.arg();
            case EVERY_N_MILLIS ->
                    (System.nanoTime() - lastSyncNanos) / 1_000_000L >= policy.arg();
            case NEVER -> false;
        };
        if (should) {
            syncNow();
        }
    }

    private void syncNow() throws IOException {
        channel.force(false);
        appendsSinceSync = 0;
        lastSyncNanos = System.nanoTime();
        dirty = false;
    }

    /** Force the active segment to disk, regardless of policy. */
    public void flush() {
        try {
            syncNow();
        } catch (IOException e) {
            throw new TsWalException(TsWalException.Kind.IO, "flush failed", e);
        }
    }

    /**
     * Replay every segment in sequence order, CRC-validating each record.
     *
     * <p>Truncation-safe: at the first short, garbage, or checksum-failing record
     * in any segment, replay stops and returns the valid prefix accumulated so
     * far. A crash that left a torn write in the active segment loses only the
     * uncommitted tail; recovery never throws on it.
     */
    public List<TsWalRecord> replay() {
        List<TsWalRecord> out = new ArrayList<>();
        try {
            for (long seq : scanSegments(dir)) {
                byte[] bytes = Files.readAllBytes(dir.resolve(segmentName(seq)));
                int offset = 0;
                while (offset + RECORD_LEN <= bytes.length) {
                    TsWalRecord rec = decodeRecord(bytes, offset);
                    if (rec == null) {
                        return out;
                    }
                    out.add(rec);
                    offset += RECORD_LEN;
                }
                if (offset < bytes.length) {
                    return out;
                }
            }
        } catch (IOException e) {
            throw new TsWalException(TsWalException.Kind.IO, "replay failed", e);
        }
        return out;
    }

    /**
     * Delete whole SEALED segments whose last record's {@code ts} is strictly
     * less than {@code cutoff}. The active segment is never touched, and a
     * segment that straddles the cutoff is kept whole. Returns the number of
     * segments removed.
     */
    public int truncateBefore(long cutoff) {
        int removed = 0;
        try {
            for (long seq : scanSegments(dir)) {
                if (seq == activeSeq) {
                    continue;
                }
                Long lastTs = lastRecordTs(dir.resolve(segmentName(seq)));
                if (lastTs != null && lastTs < cutoff) {
                    Files.deleteIfExists(dir.resolve(segmentName(seq)));
                    removed++;
                }
            }
        } catch (IOException e) {
            throw new TsWalException(TsWalException.Kind.IO, "truncateBefore failed", e);
        }
        return removed;
    }

    private static Long lastRecordTs(Path path) throws IOException {
        byte[] bytes = Files.readAllBytes(path);
        int offset = 0;
        Long last = null;
        while (offset + RECORD_LEN <= bytes.length) {
            TsWalRecord rec = decodeRecord(bytes, offset);
            if (rec == null) {
                break;
            }
            last = rec.ts();
            offset += RECORD_LEN;
        }
        return last;
    }

    /** Active segment sequence number (for diagnostics + tests). */
    public long activeSeq() {
        return activeSeq;
    }

    @Override
    public void close() {
        try {
            if (dirty) {
                channel.force(false);
            }
            channel.close();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
