package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;

import java.io.DataInput;
import java.io.EOFException;
import java.io.IOException;

/**
 * xxHash3 (64-bit) checksum reader. Block format (big-endian on disk):
 *
 * <pre>
 * u32 length
 * u8  payload[length]
 * u64 xxh3-64-of-payload
 * </pre>
 *
 * <p>Faster than CRC32C on modern CPUs - xxh3 burns about 0.3 ns/byte
 * once warm; CRC32C is closer to 0.7 ns/byte. Not designed for
 * adversarial inputs (no collision-resistance guarantee); pick
 * {@link Crc32SegmentReader} instead when the segment lives somewhere
 * an attacker can touch.
 *
 * <p>The hash is computed by {@link Xxh3}, a small self-contained
 * XXH3-style 64-bit digest. The Rust sibling uses the canonical
 * {@code xxhash-rust} crate; cross-language byte-equivalence is NOT a
 * goal for this feature (each language reads its own segments).
 */
public final class Xxh3SegmentReader {

    /** Distinguishes "checksum mismatch" from generic IO failure. */
    public static final class ChecksumMismatch extends IOException {
        public ChecksumMismatch(String msg) { super(msg); }
    }

    private final DataInput in;
    private byte[] buf = new byte[64];

    public Xxh3SegmentReader(DataInput in) {
        this.in = in;
    }

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
        long expected;
        try {
            expected = in.readLong();
        } catch (EOFException eof) {
            throw new SegmentReader.TruncatedFrame("checksum trailer truncated");
        }
        long actual = Xxh3.hash64(buf, 0, len);
        if (expected != actual) {
            throw new ChecksumMismatch("xxh3 mismatch: expected="
                    + Long.toHexString(expected) + " actual=" + Long.toHexString(actual));
        }
        byte[] out = new byte[len];
        System.arraycopy(buf, 0, out, 0, len);
        return out;
    }
}
