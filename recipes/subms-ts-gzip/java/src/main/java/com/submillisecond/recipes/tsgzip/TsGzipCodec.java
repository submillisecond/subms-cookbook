package com.submillisecond.recipes.tsgzip;

import java.io.ByteArrayOutputStream;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * A gzip codec that wraps any inner {@link TsCodec}. It hand-rolls
 * DEFLATE/INFLATE (RFC 1951) and the gzip container (RFC 1952) - no
 * {@code java.util.zip.Deflater}, nothing on the wire but bytes this class
 * wrote itself (the CRC-32 is hand-rolled too, in {@link Crc32}).
 *
 * <p>The output is a real gzip stream: the 10-byte header, a raw DEFLATE body,
 * and the CRC-32 + ISIZE trailer. That makes {@link #encode} output
 * {@code gunzip}-able, and {@link #decode} can read arbitrary gzip/zlib output
 * (stored, fixed-Huffman, AND dynamic-Huffman blocks).
 *
 * <p>Compose it over any inner codec - {@code gzip+json}, {@code gzip+cbor}.
 * The wrapper is value-type agnostic; the inner codec owns the
 * {@code TsSeries<T>} bytes shape and the wrapper only compresses the result.
 *
 * <pre>{@code
 * TsCodec<Double> codec = new TsGzipCodec<>(new TsJsonCodecAdapter(), 6);
 * byte[] bytes = codec.encode(series);   // a real gzip stream
 * TsSeries<Double> back = codec.decode(bytes);
 * }</pre>
 *
 * @param <T> the inner codec's value type
 */
public final class TsGzipCodec<T> implements TsCodec<T> {

    // 10-byte gzip header: magic 1f 8b, CM=8 (deflate), FLG=0, MTIME=0 (4),
    // XFL=0, OS=255 (unknown). MTIME stays zero so encode is deterministic.
    private static final byte[] GZIP_HEADER = {
        0x1f, (byte) 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x00, (byte) 0xff
    };

    private final TsCodec<T> inner;
    private final int level;
    private final String format;

    /**
     * @param level 0 = stored only, 1..3 = greedy LZ77, 4..9 = lazy matching
     *              with growing match-chain effort. Values above 9 clamp to 9.
     */
    public TsGzipCodec(TsCodec<T> inner, int level) {
        this.inner = inner;
        this.level = Math.min(Math.max(level, 0), 9);
        this.format = "gzip+" + inner.format();
    }

    public int level() {
        return level;
    }

    public TsCodec<T> inner() {
        return inner;
    }

    @Override
    public byte[] encode(TsSeries<T> series) {
        return gzip(inner.encode(series), level);
    }

    @Override
    public TsSeries<T> decode(byte[] bytes) {
        return inner.decode(gunzip(bytes));
    }

    @Override
    public String format() {
        return format;
    }

    /** Wrap raw bytes in a gzip container: header + DEFLATE(payload) + CRC32 + ISIZE. */
    public static byte[] gzip(byte[] payload, int level) {
        byte[] body = Deflate.deflate(payload, level);
        ByteArrayOutputStream out = new ByteArrayOutputStream(body.length + 18);
        out.write(GZIP_HEADER, 0, GZIP_HEADER.length);
        out.write(body, 0, body.length);
        long crc = Crc32.crc32(payload);
        writeLe32(out, crc);
        writeLe32(out, payload.length & 0xFFFFFFFFL);
        return out.toByteArray();
    }

    /**
     * Unwrap a gzip container and inflate the body, verifying CRC32 + ISIZE.
     * Accepts any standards-compliant gzip stream (stored / fixed / dynamic).
     */
    public static byte[] gunzip(byte[] bytes) {
        if (bytes.length < 18) {
            throw TsGzipException.truncated();
        }
        if ((bytes[0] & 0xff) != 0x1f || (bytes[1] & 0xff) != 0x8b) {
            throw TsGzipException.badMagic();
        }
        if ((bytes[2] & 0xff) != 8) {
            throw TsGzipException.badMethod(bytes[2] & 0xff);
        }
        int flg = bytes[3] & 0xff;
        if ((flg & 0xe0) != 0) {
            throw TsGzipException.unsupportedFlag(flg);
        }
        int pos = 10;
        if ((flg & 0x04) != 0) { // FEXTRA
            if (pos + 2 > bytes.length) {
                throw TsGzipException.truncated();
            }
            int xlen = (bytes[pos] & 0xff) | ((bytes[pos + 1] & 0xff) << 8);
            pos += 2 + xlen;
        }
        if ((flg & 0x08) != 0) { // FNAME
            pos = skipCstr(bytes, pos);
        }
        if ((flg & 0x10) != 0) { // FCOMMENT
            pos = skipCstr(bytes, pos);
        }
        if ((flg & 0x02) != 0) { // FHCRC
            pos += 2;
        }
        if (pos + 8 > bytes.length || pos < 0) {
            throw TsGzipException.truncated();
        }
        int t = bytes.length - 8;
        byte[] out = Inflate.inflate(bytes, pos, t - pos);

        long wantCrc = readLe32(bytes, t);
        long gotCrc = Crc32.crc32(out);
        if (wantCrc != gotCrc) {
            throw TsGzipException.crcMismatch(wantCrc, gotCrc);
        }
        long wantSize = readLe32(bytes, t + 4);
        long gotSize = out.length & 0xFFFFFFFFL;
        if (wantSize != gotSize) {
            throw TsGzipException.sizeMismatch(wantSize, gotSize);
        }
        return out;
    }

    private static int skipCstr(byte[] bytes, int pos) {
        while (pos < bytes.length) {
            byte b = bytes[pos++];
            if (b == 0) {
                return pos;
            }
        }
        throw TsGzipException.truncated();
    }

    private static void writeLe32(ByteArrayOutputStream out, long v) {
        out.write((int) (v & 0xff));
        out.write((int) ((v >>> 8) & 0xff));
        out.write((int) ((v >>> 16) & 0xff));
        out.write((int) ((v >>> 24) & 0xff));
    }

    private static long readLe32(byte[] b, int off) {
        return (b[off] & 0xffL)
                | ((b[off + 1] & 0xffL) << 8)
                | ((b[off + 2] & 0xffL) << 16)
                | ((b[off + 3] & 0xffL) << 24);
    }
}
