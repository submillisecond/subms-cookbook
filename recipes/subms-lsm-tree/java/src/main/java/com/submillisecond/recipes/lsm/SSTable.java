package com.submillisecond.recipes.lsm;

import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
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

    private SSTable(Path path, byte[] buf, int recordsEnd, BloomFilter bloom) {
        this.path = path;
        this.buf = buf;
        this.recordsEnd = recordsEnd;
        this.bloom = bloom;
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
        int p = 0;
        while (p < recordsEnd) {
            int keyLen = readIntBE(buf, p); p += 4;
            int cmp = compareKey(buf, p, keyLen, keyBytes);
            p += keyLen;
            byte flag = buf[p]; p += 1;
            int valueLen = readIntBE(buf, p); p += 4;
            if (cmp == 0) {
                if (flag == FLAG_TOMBSTONE) return Optional.of(new Hit(null));
                byte[] value = new byte[valueLen];
                System.arraycopy(buf, p, value, 0, valueLen);
                return Optional.of(new Hit(value));
            }
            if (cmp > 0) return Optional.empty();    // sorted: passed the key
            p += valueLen;
        }
        return Optional.empty();
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
