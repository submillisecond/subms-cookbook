package com.submillisecond.recipes.segment;

import java.io.DataInput;
import java.io.EOFException;
import java.io.IOException;

/** Streams length-prefix (big-endian u32 + payload) framed records. */
public final class SegmentReader {

    public static final class TruncatedFrame extends IOException {
        public TruncatedFrame(String msg) { super(msg); }
    }

    private final DataInput in;
    private byte[] buf = new byte[64];

    public SegmentReader(DataInput in) {
        this.in = in;
    }

    /**
     * Read the next record. Returns {@code null} on clean EOF;
     * throws {@link TruncatedFrame} if the segment cut mid-frame.
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
            throw new TruncatedFrame("payload truncated at tail");
        }
        byte[] out = new byte[len];
        System.arraycopy(buf, 0, out, 0, len);
        return out;
    }
}
