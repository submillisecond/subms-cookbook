package com.submillisecond.recipes.lsm;

import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import com.submillisecond.recipes.bloom.BloomFilter;

/**
 * Immutable on-disk sorted run, slurped into memory on {@link #open}.
 *
 * <p>File layout (big-endian):
 * <pre>
 *   records: (key_len:u32 key:utf-8 flag:u8 value_len:u32 value:bytes)*
 *   bloom:   bit_count:u32 k:u32 word_count:u32 (word:u64)*
 *   footer:  records_end_offset:u64 magic:u32
 *   flag := 0x00 (present) | 0x01 (tombstone, value_len == 0)
 *   magic := 0x4C534D54 ("LSMT")
 * </pre>
 *
 * On open the whole file is read into a byte buffer, the bloom filter is parsed
 * out of the trailer, and reads operate entirely against memory. A get short-circuits
 * on a bloom miss.
 */
final class SSTable {

    private static final int  MAGIC          = 0x4C534D54;
    private static final byte FLAG_VALUE     = 0x00;
    private static final byte FLAG_TOMBSTONE = 0x01;
    private static final int  FOOTER_BYTES   = 8 + 4;

    private final Path path;
    private final byte[] buf;
    private final int recordsEnd;
    private final BloomFilter bloom;
    /**
     * Start byte offset of each record, in key order (records are stored
     * sorted). Built once on {@code open} from the slurped buffer; lets
     * {@code get} and {@code range} binary-search to a key instead of scanning
     * from the file start. Costs one int per record - the on-disk format is
     * unchanged.
     */
    private final int[] offsets;

    private SSTable(Path path, byte[] buf, int recordsEnd, BloomFilter bloom) {
        this.path = path;
        this.buf = buf;
        this.recordsEnd = recordsEnd;
        this.bloom = bloom;
        this.offsets = indexOffsets(buf, recordsEnd);
    }

    static SSTable open(Path path) throws IOException {
        byte[] buf = Files.readAllBytes(path);
        if (buf.length < FOOTER_BYTES) throw new IOException("SSTable too small: " + path);

        int magicOff = buf.length - 4;
        int magic = readIntBE(buf, magicOff);
        if (magic != MAGIC) throw new IOException("bad SSTable magic in " + path);

        long recordsEndLong = readLongBE(buf, buf.length - FOOTER_BYTES);
        if (recordsEndLong < 0 || recordsEndLong > buf.length - FOOTER_BYTES) {
            throw new IOException("bad records_end offset in " + path);
        }
        int recordsEnd = (int) recordsEndLong;
        int bloomLen = buf.length - FOOTER_BYTES - recordsEnd;
        BloomFilter bloom = BloomFilter.parse(buf, recordsEnd, bloomLen);
        return new SSTable(path, buf, recordsEnd, bloom);
    }

    /** One pass over the sorted records collecting each record's start offset. */
    private static int[] indexOffsets(byte[] buf, int recordsEnd) {
        int count = 0;
        int p = 0;
        while (p < recordsEnd) {
            count++;
            int keyLen = readIntBE(buf, p);
            p += 4 + keyLen + 1;
            int valueLen = readIntBE(buf, p);
            p += 4 + valueLen;
        }
        int[] offs = new int[count];
        p = 0;
        for (int i = 0; i < count; i++) {
            offs[i] = p;
            int keyLen = readIntBE(buf, p);
            p += 4 + keyLen + 1;
            int valueLen = readIntBE(buf, p);
            p += 4 + valueLen;
        }
        return offs;
    }

    /** Compare the key of the record at {@code off} against {@code keyBytes}. */
    private int compareKeyAt(int off, byte[] keyBytes) {
        int keyLen = readIntBE(buf, off);
        return compareKey(buf, off + 4, keyLen, keyBytes);
    }

    static SSTable write(
        Path path,
        int expectedEntries,
        Iterable<Map.Entry<String, byte[]>> sortedEntries
    ) throws IOException {
        BloomFilter bloom = new BloomFilter(expectedEntries);
        long recordsEnd = 0;
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(path)))) {
            for (Map.Entry<String, byte[]> e : sortedEntries) {
                bloom.add(e.getKey());
                byte[] keyBytes = e.getKey().getBytes(StandardCharsets.UTF_8);
                byte[] value = e.getValue();
                out.writeInt(keyBytes.length);
                out.write(keyBytes);
                if (value == null) {
                    out.writeByte(FLAG_TOMBSTONE);
                    out.writeInt(0);
                    recordsEnd += 4L + keyBytes.length + 1 + 4;
                } else {
                    out.writeByte(FLAG_VALUE);
                    out.writeInt(value.length);
                    out.write(value);
                    recordsEnd += 4L + keyBytes.length + 1 + 4 + value.length;
                }
            }
            bloom.writeTo(out);
            out.writeLong(recordsEnd);
            out.writeInt(MAGIC);
        }
        return open(path);
    }

    /**
     * @return {@code Optional.empty()} if the key is not in this run;
     *         a hit carrying {@code null} value if it is tombstoned;
     *         a hit carrying the value otherwise.
     *
     *         {@code checkBloom = false} skips the bloom probe and goes
     *         straight to the scan - used by {@link BloomMode#OFF} to
     *         measure the optimisation's value.
     */
    Optional<Hit> get(String key, boolean checkBloom) {
        if (checkBloom && !bloom.mightContain(key)) return Optional.empty();

        byte[] keyBytes = key.getBytes(StandardCharsets.UTF_8);
        // Records are sorted, so binary-search the offset index (O(log n)) rather
        // than scan from the file start.
        int lo = 0, hi = offsets.length - 1;
        while (lo <= hi) {
            int mid = (lo + hi) >>> 1;
            int off = offsets[mid];
            int cmp = compareKeyAt(off, keyBytes);
            if (cmp == 0) {
                int p = off + 4 + readIntBE(buf, off);
                byte flag = buf[p]; p += 1;
                int valueLen = readIntBE(buf, p); p += 4;
                if (flag == FLAG_TOMBSTONE) return Optional.of(new Hit(null));
                byte[] value = new byte[valueLen];
                System.arraycopy(buf, p, value, 0, valueLen);
                return Optional.of(new Hit(value));
            }
            if (cmp < 0) lo = mid + 1; else hi = mid - 1;
        }
        return Optional.empty();
    }

    /**
     * Records whose key is in {@code [lo, hi)} (a {@code null} bound is
     * unbounded), in key order, as {@code (key, value)} entries - a tombstone
     * surfaces as an entry with a {@code null} value. Binary-searches the offset
     * index to seek to {@code lo}, then scans forward only across the window.
     */
    List<Map.Entry<String, byte[]>> range(String lo, String hi) {
        byte[] loBytes = lo == null ? null : lo.getBytes(StandardCharsets.UTF_8);
        byte[] hiBytes = hi == null ? null : hi.getBytes(StandardCharsets.UTF_8);
        // Lower bound: first record whose key is >= lo.
        int start = 0;
        if (loBytes != null) {
            int a = 0, b = offsets.length;
            while (a < b) {
                int mid = (a + b) >>> 1;
                if (compareKeyAt(offsets[mid], loBytes) < 0) a = mid + 1;
                else b = mid;
            }
            start = a;
        }
        List<Map.Entry<String, byte[]>> out = new ArrayList<>();
        for (int i = start; i < offsets.length; i++) {
            int off = offsets[i];
            int keyLen = readIntBE(buf, off);
            int keyOff = off + 4;
            if (hiBytes != null && compareKey(buf, keyOff, keyLen, hiBytes) >= 0) break;
            int p = keyOff + keyLen;
            byte flag = buf[p]; p += 1;
            int valueLen = readIntBE(buf, p); p += 4;
            String key = new String(buf, keyOff, keyLen, StandardCharsets.UTF_8);
            byte[] value = null;
            if (flag != FLAG_TOMBSTONE) {
                value = new byte[valueLen];
                System.arraycopy(buf, p, value, 0, valueLen);
            }
            out.add(new AbstractMap.SimpleImmutableEntry<>(key, value));
        }
        return out;
    }

    Path path() {
        return path;
    }

    private static int compareKey(byte[] buf, int off, int len, byte[] keyBytes) {
        int n = Math.min(len, keyBytes.length);
        for (int i = 0; i < n; i++) {
            int a = buf[off + i] & 0xff;
            int b = keyBytes[i] & 0xff;
            if (a != b) return a - b;
        }
        return Integer.compare(len, keyBytes.length);
    }

    private static int readIntBE(byte[] b, int o) {
        return ((b[o] & 0xff) << 24)
             | ((b[o + 1] & 0xff) << 16)
             | ((b[o + 2] & 0xff) << 8)
             |  (b[o + 3] & 0xff);
    }

    private static long readLongBE(byte[] b, int o) {
        long hi = (long) readIntBE(b, o) & 0xffffffffL;
        long lo = (long) readIntBE(b, o + 4) & 0xffffffffL;
        return (hi << 32) | lo;
    }

    record Hit(byte[] value) {
        boolean isTombstone() {
            return value == null;
        }
    }
}
