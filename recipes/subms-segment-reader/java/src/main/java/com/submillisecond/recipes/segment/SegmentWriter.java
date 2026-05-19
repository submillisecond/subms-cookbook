package com.submillisecond.recipes.segment;

import java.io.DataOutput;
import java.io.IOException;

/** Writes length-prefix (big-endian u32 + payload) framed records. */
public final class SegmentWriter {
    private final DataOutput out;
    public SegmentWriter(DataOutput out) { this.out = out; }

    public void write(byte[] record) throws IOException {
        out.writeInt(record.length);
        out.write(record);
    }
}
