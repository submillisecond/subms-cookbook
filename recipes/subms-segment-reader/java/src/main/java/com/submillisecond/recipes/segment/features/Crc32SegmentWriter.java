package com.submillisecond.recipes.segment.features;

import java.io.DataOutput;
import java.io.IOException;
import java.util.zip.CRC32C;

/**
 * Writes CRC32C-checked blocks matching {@link Crc32SegmentReader}.
 * Block format: {@code u32 length | payload | u32 crc32c-of-payload}.
 */
public final class Crc32SegmentWriter {

    private final DataOutput out;

    public Crc32SegmentWriter(DataOutput out) {
        this.out = out;
    }

    public void write(byte[] record) throws IOException {
        out.writeInt(record.length);
        out.write(record);
        CRC32C crc = new CRC32C();
        crc.update(record, 0, record.length);
        out.writeInt((int) crc.getValue());
    }
}
