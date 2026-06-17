package com.submillisecond.recipes.segment.features;

import net.jpountz.lz4.LZ4Compressor;
import net.jpountz.lz4.LZ4Factory;

import java.io.DataOutput;
import java.io.IOException;

/**
 * Block writer matched to {@link Lz4SegmentReader}. Picks {@code stored}
 * when the LZ4 output would be larger than the raw payload (small /
 * random blocks), and {@code lz4} otherwise.
 */
public final class Lz4BlockWriter {

    private final DataOutput out;
    private final LZ4Compressor compressor;

    public Lz4BlockWriter(DataOutput out) {
        this.out = out;
        this.compressor = LZ4Factory.fastestInstance().fastCompressor();
    }

    /** Write a block, picking the smaller of stored / lz4 encodings. */
    public void write(byte[] payload) throws IOException {
        int max = compressor.maxCompressedLength(payload.length);
        byte[] buf = new byte[max];
        int n = compressor.compress(payload, 0, payload.length, buf, 0, max);
        if (n < payload.length) {
            writeBlock(Lz4SegmentReader.TAG_LZ4, payload.length, buf, n);
        } else {
            writeBlock(Lz4SegmentReader.TAG_STORED, payload.length, payload, payload.length);
        }
    }

    /** Force LZ4 even when stored would be smaller. Useful for tests. */
    public void writeLz4(byte[] payload) throws IOException {
        int max = compressor.maxCompressedLength(payload.length);
        byte[] buf = new byte[max];
        int n = compressor.compress(payload, 0, payload.length, buf, 0, max);
        writeBlock(Lz4SegmentReader.TAG_LZ4, payload.length, buf, n);
    }

    /** Force stored. */
    public void writeStored(byte[] payload) throws IOException {
        writeBlock(Lz4SegmentReader.TAG_STORED, payload.length, payload, payload.length);
    }

    private void writeBlock(byte tag, int uncompressedLen, byte[] body, int bodyLen) throws IOException {
        out.writeByte(tag);
        out.writeInt(uncompressedLen);
        out.writeInt(bodyLen);
        out.write(body, 0, bodyLen);
    }
}
