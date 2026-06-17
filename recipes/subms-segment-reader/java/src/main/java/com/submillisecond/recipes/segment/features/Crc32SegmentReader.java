package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;

import java.io.DataInput;
import java.io.EOFException;
import java.io.IOException;
import java.util.zip.CRC32C;

/**
 * CRC32C-checked segment reader. Block format (big-endian on disk):
 *
 * <pre>
 * u32 length
 * u8  payload[length]
 * u32 crc32c-of-payload
 * </pre>
 *
 * <p>Uses {@link CRC32C} (Castagnoli polynomial, JDK 9+). The JIT lowers
 * it to SSE4.2 / ARMv8 CRC instructions where available, otherwise a
 * software table. Byte-equivalent to the Rust sibling using the
 * {@code crc32c} crate.
 */
public final class Crc32SegmentReader {

    /** Distinguishes "checksum mismatch" from generic IO failure. */
    public static final class ChecksumMismatch extends IOException {
        public ChecksumMismatch(String msg) { super(msg); }
    }

    private final DataInput in;
    private byte[] buf = new byte[64];

    public Crc32SegmentReader(DataInput in) {
        this.in = in;
    }

    /**
     * Read the next CRC-checked record. Returns {@code null} on clean EOF,
     * throws {@link SegmentReader.TruncatedFrame} mid-block, or
     * {@link ChecksumMismatch} when the trailer doesn't match.
     */
    public byte[] nextRecord() throws IOException {
        int len;
        try {
            len = in.readInt();
        } catch (EOFException eof) {
            return null;
        }
        if (len < 0) throw new IOException("negative frame length: " + len);
        if (buf.length < len) buf = new byte[len];
        try {
            in.readFully(buf, 0, len);
        } catch (EOFException eof) {
            throw new SegmentReader.TruncatedFrame("payload truncated at tail");
        }
        int expected;
        try {
            expected = in.readInt();
        } catch (EOFException eof) {
            throw new SegmentReader.TruncatedFrame("checksum trailer truncated");
        }
        CRC32C crc = new CRC32C();
        crc.update(buf, 0, len);
        int actual = (int) crc.getValue();
        if (expected != actual) {
            throw new ChecksumMismatch("crc32c mismatch: expected="
                    + Integer.toHexString(expected) + " actual=" + Integer.toHexString(actual));
        }
        byte[] out = new byte[len];
        System.arraycopy(buf, 0, out, 0, len);
        return out;
    }
}
