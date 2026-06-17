package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import net.jpountz.lz4.LZ4Factory;
import net.jpountz.lz4.LZ4FastDecompressor;

import java.io.DataInput;
import java.io.EOFException;
import java.io.IOException;

/**
 * LZ4 block decompression. Block format (big-endian on disk):
 *
 * <pre>
 * u8  algo_tag       (0 = stored, 1 = lz4)
 * u32 uncompressed_len
 * u32 compressed_len
 * u8  payload[compressed_len]
 * </pre>
 *
 * <p>A {@code 0} tag means the payload is stored verbatim (writer chose
 * not to compress, typically because the block was incompressible).
 * A {@code 1} tag means LZ4-compressed; the uncompressed length is held
 * out-of-band in the header so the decoder doesn't need to look at the
 * canonical LZ4 frame format. Any other tag throws
 * {@link DecompressionFailed}.
 *
 * <p>Requires the {@code org.lz4:lz4-java} optional dependency.
 */
public final class Lz4SegmentReader {

    public static final byte TAG_STORED = 0;
    public static final byte TAG_LZ4 = 1;

    /** Distinguishes "bad block" from generic IO failure. */
    public static final class DecompressionFailed extends IOException {
        public DecompressionFailed(String msg) { super(msg); }
        public DecompressionFailed(String msg, Throwable cause) { super(msg, cause); }
    }

    private final DataInput in;
    private final LZ4FastDecompressor decompressor;

    public Lz4SegmentReader(DataInput in) {
        this.in = in;
        this.decompressor = LZ4Factory.fastestInstance().fastDecompressor();
    }

    public byte[] nextRecord() throws IOException {
        byte tag;
        try {
            tag = in.readByte();
        } catch (EOFException eof) {
            return null;
        }
        int uncompressedLen;
        int compressedLen;
        try {
            uncompressedLen = in.readInt();
            compressedLen = in.readInt();
        } catch (EOFException eof) {
            throw new SegmentReader.TruncatedFrame("block header truncated");
        }
        if (uncompressedLen < 0 || compressedLen < 0) {
            throw new IOException("negative block length: u=" + uncompressedLen + " c=" + compressedLen);
        }
        byte[] payload = new byte[compressedLen];
        try {
            in.readFully(payload);
        } catch (EOFException eof) {
            throw new SegmentReader.TruncatedFrame("compressed payload truncated");
        }
        switch (tag) {
            case TAG_STORED: {
                if (uncompressedLen != compressedLen) {
                    throw new DecompressionFailed("stored block size mismatch: u="
                            + uncompressedLen + " c=" + compressedLen);
                }
                return payload;
            }
            case TAG_LZ4: {
                byte[] out = new byte[uncompressedLen];
                try {
                    int consumed = decompressor.decompress(payload, 0, out, 0, uncompressedLen);
                    if (consumed != compressedLen) {
                        throw new DecompressionFailed("lz4 consumed " + consumed + " of " + compressedLen + " bytes");
                    }
                } catch (RuntimeException re) {
                    throw new DecompressionFailed("lz4 decompress failed", re);
                }
                return out;
            }
            default:
                throw new DecompressionFailed("unknown algo tag: " + (tag & 0xff));
        }
    }
}
