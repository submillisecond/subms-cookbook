package com.submillisecond.recipes.lsm.features;

import java.io.IOException;
import java.nio.ByteBuffer;

import net.jpountz.lz4.LZ4Compressor;
import net.jpountz.lz4.LZ4Factory;
import net.jpountz.lz4.LZ4FastDecompressor;

/**
 * LZ4 block compression for SSTable data blocks.
 *
 * <p>Block format (big-endian) mirrors the Rust sibling so a file written by
 * one side can be decoded by the other:
 * <pre>
 *   marker:    u8   = 0x4C ('L')
 *   algo:      u8   = 0x00 (stored) | 0x01 (lz4)
 *   uncomp:    u32  uncompressed byte length
 *   data:      bytes
 * </pre>
 *
 * <p>Built on {@code lz4-java} (declared {@code &lt;optional&gt;true&lt;/optional&gt;}
 * in the pom). Falls back to a stored-as-is encoding when LZ4 doesn't shrink
 * the input - the algo byte discriminates between {@code lz4} (0x01) and
 * {@code stored} (0x00). Decompression handles both.
 */
public final class Lz4BlockCompressor {

    private static final byte MARKER       = 0x4C;
    private static final byte ALGO_STORED  = 0x00;
    private static final byte ALGO_LZ4     = 0x01;
    private static final int  HEADER_LEN   = 1 + 1 + 4;

    private final LZ4Compressor compressor;
    private final LZ4FastDecompressor decompressor;

    public Lz4BlockCompressor() {
        LZ4Factory factory = LZ4Factory.fastestInstance();
        this.compressor = factory.fastCompressor();
        this.decompressor = factory.fastDecompressor();
    }

    public byte[] compress(byte[] block) {
        int maxLen = compressor.maxCompressedLength(block.length);
        byte[] scratch = new byte[maxLen];
        int n = compressor.compress(block, 0, block.length, scratch, 0, maxLen);
        ByteBuffer out;
        if (n < block.length) {
            out = ByteBuffer.allocate(HEADER_LEN + n);
            out.put(MARKER);
            out.put(ALGO_LZ4);
            out.putInt(block.length);
            out.put(scratch, 0, n);
        } else {
            out = ByteBuffer.allocate(HEADER_LEN + block.length);
            out.put(MARKER);
            out.put(ALGO_STORED);
            out.putInt(block.length);
            out.put(block);
        }
        return out.array();
    }

    public byte[] decompress(byte[] buf) throws IOException {
        if (buf.length < HEADER_LEN) {
            throw new IOException("lz4 block: too short");
        }
        if (buf[0] != MARKER) {
            throw new IOException("lz4 block: bad marker");
        }
        byte algo = buf[1];
        int uncompLen = ((buf[2] & 0xff) << 24)
                      | ((buf[3] & 0xff) << 16)
                      | ((buf[4] & 0xff) << 8)
                      |  (buf[5] & 0xff);
        int payloadLen = buf.length - HEADER_LEN;
        switch (algo) {
            case ALGO_STORED -> {
                if (payloadLen != uncompLen) {
                    throw new IOException("lz4 block: stored payload size mismatch");
                }
                byte[] out = new byte[uncompLen];
                System.arraycopy(buf, HEADER_LEN, out, 0, uncompLen);
                return out;
            }
            case ALGO_LZ4 -> {
                byte[] out = new byte[uncompLen];
                try {
                    decompressor.decompress(buf, HEADER_LEN, out, 0, uncompLen);
                } catch (RuntimeException ex) {
                    throw new IOException("lz4 decode: " + ex.getMessage(), ex);
                }
                return out;
            }
            default -> throw new IOException(
                    "lz4 block: unknown algo byte 0x" + Integer.toHexString(algo & 0xff));
        }
    }
}
