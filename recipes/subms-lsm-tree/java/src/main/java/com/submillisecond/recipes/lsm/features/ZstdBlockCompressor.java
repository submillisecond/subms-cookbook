package com.submillisecond.recipes.lsm.features;

import java.io.IOException;
import java.nio.ByteBuffer;

import com.github.luben.zstd.Zstd;

/**
 * Zstd block compression for SSTable data blocks. Mirrors the LZ4 wrapper's
 * header so the read path can dispatch by the algo byte if multiple
 * compressors share a file:
 * <pre>
 *   marker:    u8   = 0x5A ('Z')
 *   algo:      u8   = 0x00 (stored) | 0x01 (zstd)
 *   uncomp:    u32  uncompressed byte length
 *   data:      bytes
 * </pre>
 *
 * <p>Compression level defaults to 3. Use {@link #withLevel(int)} to override.
 * Levels outside [1, 22] are clamped.
 *
 * <p>{@code zstd-jni} is declared {@code &lt;optional&gt;true&lt;/optional&gt;}
 * in the pom; downstream consumers must pull it in explicitly.
 */
public final class ZstdBlockCompressor {

    private static final byte MARKER      = 0x5A;
    private static final byte ALGO_STORED = 0x00;
    private static final byte ALGO_ZSTD   = 0x01;
    private static final int  HEADER_LEN  = 1 + 1 + 4;
    private static final int  DEFAULT_LEVEL = 3;
    private static final int  MIN_LEVEL = 1;
    private static final int  MAX_LEVEL = 22;

    private final int level;

    public ZstdBlockCompressor() {
        this.level = DEFAULT_LEVEL;
    }

    private ZstdBlockCompressor(int level) {
        this.level = Math.max(MIN_LEVEL, Math.min(MAX_LEVEL, level));
    }

    public static ZstdBlockCompressor withLevel(int level) {
        return new ZstdBlockCompressor(level);
    }

    public int level() {
        return level;
    }

    public byte[] compress(byte[] block) throws IOException {
        long maxLen = Zstd.compressBound(block.length);
        byte[] scratch = new byte[Math.toIntExact(maxLen)];
        long n = Zstd.compressByteArray(scratch, 0, scratch.length, block, 0, block.length, level);
        if (Zstd.isError(n)) {
            throw new IOException("zstd compress: " + Zstd.getErrorName(n));
        }
        int compLen = Math.toIntExact(n);
        ByteBuffer out;
        if (compLen < block.length) {
            out = ByteBuffer.allocate(HEADER_LEN + compLen);
            out.put(MARKER);
            out.put(ALGO_ZSTD);
            out.putInt(block.length);
            out.put(scratch, 0, compLen);
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
            throw new IOException("zstd block: too short");
        }
        if (buf[0] != MARKER) {
            throw new IOException("zstd block: bad marker");
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
                    throw new IOException("zstd block: stored payload size mismatch");
                }
                byte[] out = new byte[uncompLen];
                System.arraycopy(buf, HEADER_LEN, out, 0, uncompLen);
                return out;
            }
            case ALGO_ZSTD -> {
                byte[] out = new byte[uncompLen];
                long n = Zstd.decompressByteArray(out, 0, uncompLen, buf, HEADER_LEN, payloadLen);
                if (Zstd.isError(n)) {
                    throw new IOException("zstd decompress: " + Zstd.getErrorName(n));
                }
                if ((int) n != uncompLen) {
                    throw new IOException("zstd block: decoded length mismatch");
                }
                return out;
            }
            default -> throw new IOException(
                    "zstd block: unknown algo byte 0x" + Integer.toHexString(algo & 0xff));
        }
    }
}
