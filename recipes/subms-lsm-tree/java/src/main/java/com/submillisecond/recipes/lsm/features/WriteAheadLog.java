package com.submillisecond.recipes.lsm.features;

import java.io.IOException;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.zip.CRC32;

/**
 * Write-ahead log: every memtable mutation is appended to a wal file before
 * returning. On reload the wal replays back into a fresh memtable; a flush
 * of the memtable then truncates the wal.
 *
 * <p>File layout (big-endian, append-only):
 * <pre>
 *   record := op:u8  key_len:u32  key:utf-8  value_len:u32  value:bytes  crc:u32
 *   op     := 0x00 (put) | 0x01 (delete)
 * </pre>
 *
 * <p>The CRC32 covers {@code op | key | value} and lets the replay path tear
 * off a half-written tail without poisoning the recovered state. A torn
 * record is treated as end-of-log.
 *
 * <p>Single-writer by construction. Byte-equivalent to the Rust sibling
 * {@code subms_lsm_tree::features::wal::WriteAheadLog}.
 */
public final class WriteAheadLog implements AutoCloseable {

    private static final byte OP_PUT = 0x00;
    private static final byte OP_DELETE = 0x01;

    private final Path path;
    private OutputStream writer;

    public WriteAheadLog(Path path) throws IOException {
        this.path = path;
        this.writer = openAppend(path);
    }

    private static OutputStream openAppend(Path p) throws IOException {
        if (!Files.exists(p)) {
            Files.createFile(p);
        }
        OpenOption[] opts = {StandardOpenOption.WRITE, StandardOpenOption.APPEND};
        return Files.newOutputStream(p, opts);
    }

    public Path path() {
        return path;
    }

    /** Append a put record. Returns after the record is buffered;
     *  call {@link #sync()} for durability. */
    public void logPut(String key, byte[] value) throws IOException {
        append(OP_PUT, key.getBytes(StandardCharsets.UTF_8), value);
    }

    /** Append a delete record (tombstone). Value bytes are empty. */
    public void logDelete(String key) throws IOException {
        append(OP_DELETE, key.getBytes(StandardCharsets.UTF_8), new byte[0]);
    }

    private void append(byte op, byte[] key, byte[] value) throws IOException {
        CRC32 crc = new CRC32();
        crc.update(new byte[]{op});
        crc.update(key);
        crc.update(value);
        int checksum = (int) crc.getValue();

        ByteBuffer buf = ByteBuffer.allocate(1 + 4 + key.length + 4 + value.length + 4);
        buf.put(op);
        buf.putInt(key.length);
        buf.put(key);
        buf.putInt(value.length);
        buf.put(value);
        buf.putInt(checksum);
        writer.write(buf.array());
        writer.flush();
    }

    /** fsync analogue: flush + force durability. */
    public void sync() throws IOException {
        writer.flush();
        // OutputStream from newOutputStream does not expose sync; force via channel.
        try (var ch = java.nio.channels.FileChannel.open(path, StandardOpenOption.WRITE)) {
            ch.force(true);
        }
    }

    /**
     * Truncate the wal back to zero. Call after a successful memtable flush -
     * the SSTable now owns the records' durability. Closes and reopens the
     * underlying file so the operation works regardless of append-mode quirks.
     */
    public void truncate() throws IOException {
        writer.close();
        Files.write(path, new byte[0],
                StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING);
        writer = openAppend(path);
    }

    @Override
    public void close() throws IOException {
        writer.close();
    }

    /**
     * Replay every well-formed record from the wal at {@code path}. A torn
     * final record (truncated mid-write or with a bad CRC) is treated as
     * end-of-log and silently dropped; the surviving prefix is returned in
     * insertion order.
     */
    public static List<WalEntry> replay(Path path) throws IOException {
        List<WalEntry> entries = new ArrayList<>();
        if (!Files.exists(path)) return entries;
        byte[] buf = Files.readAllBytes(path);
        int p = 0;
        while (p < buf.length) {
            if (p + 5 > buf.length) break;
            byte op = buf[p];
            int keyLen = readIntBE(buf, p + 1);
            int afterKey = p + 5 + keyLen;
            if (afterKey + 4 > buf.length) break;
            int valueLen = readIntBE(buf, afterKey);
            int afterValue = afterKey + 4 + valueLen;
            if (afterValue + 4 > buf.length) break;

            byte[] key = new byte[keyLen];
            System.arraycopy(buf, p + 5, key, 0, keyLen);
            byte[] value = new byte[valueLen];
            System.arraycopy(buf, afterKey + 4, value, 0, valueLen);
            int storedCrc = readIntBE(buf, afterValue);

            CRC32 crc = new CRC32();
            crc.update(new byte[]{op});
            crc.update(key);
            crc.update(value);
            if ((int) crc.getValue() != storedCrc) break; // torn / corrupt

            byte[] valueOrNull;
            if (op == OP_PUT) {
                valueOrNull = value;
            } else if (op == OP_DELETE) {
                valueOrNull = null;
            } else {
                break;
            }
            entries.add(new WalEntry(new String(key, StandardCharsets.UTF_8), valueOrNull));
            p = afterValue + 4;
        }
        return entries;
    }

    private static int readIntBE(byte[] b, int o) {
        return ((b[o] & 0xff) << 24)
             | ((b[o + 1] & 0xff) << 16)
             | ((b[o + 2] & 0xff) << 8)
             |  (b[o + 3] & 0xff);
    }

    /**
     * A replayed entry. {@code value == null} for a delete (tombstone),
     * non-null bytes for a put.
     */
    public record WalEntry(String key, byte[] value) {
        public boolean isDelete() {
            return value == null;
        }
    }
}
