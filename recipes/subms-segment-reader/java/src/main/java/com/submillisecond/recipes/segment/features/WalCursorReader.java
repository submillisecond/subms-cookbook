package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;

import java.io.IOException;

/**
 * WAL-style cursor reader.
 *
 * <p>Wraps a byte-array segment and tracks a {@code committedOffset}
 * byte watermark. {@link #readCommitted()} returns the next block only
 * if its tail lies at or before the watermark - readers see exactly
 * the prefix the writer last fsync'd, never partial post-fsync data.
 *
 * <p>Watermarks are monotonic: backward {@link #setCommitted(int)}
 * calls are rejected silently. {@link #nextRecord()} (the dirty read)
 * ignores the watermark, intended for crash-recovery / forensic paths.
 */
public final class WalCursorReader {

    private final byte[] buf;
    private int pos;
    private int committed;

    public WalCursorReader(byte[] buf) {
        this.buf = buf;
        this.pos = 0;
        this.committed = 0;
    }

    public WalCursorReader(byte[] buf, int committed) {
        this.buf = buf;
        this.pos = 0;
        this.committed = Math.min(committed, buf.length);
    }

    public void setCommitted(int offset) {
        int clamped = Math.min(offset, buf.length);
        if (clamped > committed) committed = clamped;
    }

    public int committed() { return committed; }
    public int position() { return pos; }

    /** Read the next block iff its tail lies at-or-before the committed
     *  watermark. Returns {@code null} at EOF OR when the next block
     *  would step past the watermark. */
    public byte[] readCommitted() throws IOException {
        if (pos == buf.length) return null;
        if (pos + 4 > buf.length) {
            throw new SegmentReader.TruncatedFrame("header truncated at offset " + pos);
        }
        int len = readBigEndianInt(pos);
        if (len < 0) throw new IOException("negative frame length at " + pos + ": " + len);
        int payloadStart = pos + 4;
        int payloadEnd = payloadStart + len;
        if (payloadEnd > buf.length) {
            throw new SegmentReader.TruncatedFrame("payload truncated at offset " + pos);
        }
        if (payloadEnd > committed) return null;
        byte[] out = new byte[len];
        System.arraycopy(buf, payloadStart, out, 0, len);
        pos = payloadEnd;
        return out;
    }

    /** Dirty read - ignore the watermark. */
    public byte[] nextRecord() throws IOException {
        if (pos == buf.length) return null;
        if (pos + 4 > buf.length) {
            throw new SegmentReader.TruncatedFrame("header truncated at offset " + pos);
        }
        int len = readBigEndianInt(pos);
        if (len < 0) throw new IOException("negative frame length at " + pos + ": " + len);
        int payloadStart = pos + 4;
        int payloadEnd = payloadStart + len;
        if (payloadEnd > buf.length) {
            throw new SegmentReader.TruncatedFrame("payload truncated at offset " + pos);
        }
        byte[] out = new byte[len];
        System.arraycopy(buf, payloadStart, out, 0, len);
        pos = payloadEnd;
        return out;
    }

    private int readBigEndianInt(int o) {
        return ((buf[o] & 0xff) << 24)
             | ((buf[o + 1] & 0xff) << 16)
             | ((buf[o + 2] & 0xff) << 8)
             | (buf[o + 3] & 0xff);
    }
}
