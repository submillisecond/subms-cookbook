package com.submillisecond.recipes.segment.features;

import java.io.DataOutput;
import java.io.IOException;

/**
 * Writes XXH3-checked blocks matching {@link Xxh3SegmentReader}.
 * Block format: {@code u32 length | payload | u64 xxh3-64-of-payload}.
 */
public final class Xxh3SegmentWriter {

    private final DataOutput out;

    public Xxh3SegmentWriter(DataOutput out) {
        this.out = out;
    }

    public void write(byte[] record) throws IOException {
        out.writeInt(record.length);
        out.write(record);
        out.writeLong(Xxh3.hash64(record, 0, record.length));
    }
}
