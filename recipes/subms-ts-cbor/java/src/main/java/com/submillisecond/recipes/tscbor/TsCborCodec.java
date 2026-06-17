package com.submillisecond.recipes.tscbor;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Zero-dep CBOR codec for {@code TsSeries<Double>}. Implements the
 * {@link TsCodec} substrate from {@code subms-ts} with a compact, deterministic
 * columnar encoding: a 2-key map {@code {"ts": [..], "v": [..]}}, timestamps as
 * CBOR integers and values as IEEE-754 float64.
 *
 * <p>The encoding is canonical - fixed key order, minimal-width integer heads,
 * definite-length arrays - so the bytes are byte-equivalent to the Rust port:
 * a series encoded in one decodes byte-for-byte in the other. Like the columnar
 * JSON codec, this carries the data columns only; series metadata is not part
 * of the wire.
 */
public final class TsCborCodec implements TsCodec<Double> {

    private static final int MT_UINT = 0;
    private static final int MT_NINT = 1;
    private static final int MT_TEXT = 3;
    private static final int MT_ARRAY = 4;
    private static final int MT_MAP = 5;
    private static final int F64_HEAD = 0xfb;

    public TsCborCodec() {
    }

    @Override
    public byte[] encode(TsSeries<Double> series) {
        int n = series.size();
        ByteArrayOutputStream out = new ByteArrayOutputStream(2 + n * 13);
        writeHead(out, MT_MAP, 2);
        // fixed key order: "ts" then "v" - the canonical layout the Rust port
        // mirrors, so the bytes match.
        writeText(out, "ts");
        writeHead(out, MT_ARRAY, n);
        for (TsPoint<Double> p : series) {
            writeInt(out, p.ts());
        }
        writeText(out, "v");
        writeHead(out, MT_ARRAY, n);
        for (TsPoint<Double> p : series) {
            out.write(F64_HEAD);
            writeBe64(out, Double.doubleToLongBits(p.value()));
        }
        return out.toByteArray();
    }

    @Override
    public TsSeries<Double> decode(byte[] bytes) {
        Reader r = new Reader(bytes);
        long pairs = r.readHead(MT_MAP);
        long[] ts = null;
        double[] vals = null;
        for (long i = 0; i < pairs; i++) {
            String key = r.readText();
            switch (key) {
                case "ts" -> {
                    int len = (int) r.readHead(MT_ARRAY);
                    long[] col = new long[len];
                    for (int j = 0; j < len; j++) {
                        col[j] = r.readInt();
                    }
                    ts = col;
                }
                case "v" -> {
                    int len = (int) r.readHead(MT_ARRAY);
                    double[] col = new double[len];
                    for (int j = 0; j < len; j++) {
                        col[j] = r.readF64();
                    }
                    vals = col;
                }
                default -> throw TsCborException.unexpected("map key " + key);
            }
        }
        if (ts == null) {
            throw TsCborException.unexpected("missing ts column");
        }
        if (vals == null) {
            throw TsCborException.unexpected("missing v column");
        }
        if (ts.length != vals.length) {
            throw TsCborException.unexpected(
                    "ts (" + ts.length + ") and v (" + vals.length + ") length mismatch");
        }
        TsSeries<Double> s = TsSeries.withCapacity(ts.length);
        for (int i = 0; i < ts.length; i++) {
            try {
                s.push(ts[i], vals[i]);
            } catch (RuntimeException e) {
                throw TsCborException.unexpected(e.getMessage());
            }
        }
        return s;
    }

    @Override
    public String format() {
        return "cbor";
    }

    private static void writeHead(ByteArrayOutputStream out, int major, long n) {
        int mt = major << 5;
        if (n < 24) {
            out.write(mt | (int) n);
        } else if (n < 0x100L) {
            out.write(mt | 24);
            out.write((int) n & 0xff);
        } else if (n < 0x1_0000L) {
            out.write(mt | 25);
            out.write((int) (n >>> 8) & 0xff);
            out.write((int) n & 0xff);
        } else if (n < 0x1_0000_0000L) {
            out.write(mt | 26);
            writeBe32(out, n);
        } else {
            out.write(mt | 27);
            writeBe64(out, n);
        }
    }

    private static void writeInt(ByteArrayOutputStream out, long v) {
        if (v >= 0) {
            writeHead(out, MT_UINT, v);
        } else {
            // CBOR negative int encodes -(n + 1); -1 - v is the unsigned payload.
            writeHead(out, MT_NINT, -1 - v);
        }
    }

    private static void writeText(ByteArrayOutputStream out, String s) {
        byte[] b = s.getBytes(StandardCharsets.UTF_8);
        writeHead(out, MT_TEXT, b.length);
        out.write(b, 0, b.length);
    }

    private static void writeBe32(ByteArrayOutputStream out, long n) {
        out.write((int) (n >>> 24) & 0xff);
        out.write((int) (n >>> 16) & 0xff);
        out.write((int) (n >>> 8) & 0xff);
        out.write((int) n & 0xff);
    }

    private static void writeBe64(ByteArrayOutputStream out, long n) {
        out.write((int) (n >>> 56) & 0xff);
        out.write((int) (n >>> 48) & 0xff);
        out.write((int) (n >>> 40) & 0xff);
        out.write((int) (n >>> 32) & 0xff);
        out.write((int) (n >>> 24) & 0xff);
        out.write((int) (n >>> 16) & 0xff);
        out.write((int) (n >>> 8) & 0xff);
        out.write((int) n & 0xff);
    }

    private static final class Reader {
        private final byte[] buf;
        private int pos;

        Reader(byte[] buf) {
            this.buf = buf;
        }

        private int readByte() {
            if (pos >= buf.length) {
                throw TsCborException.truncated();
            }
            return buf[pos++] & 0xff;
        }

        private void requireRemaining(int n) {
            if (pos + n > buf.length || pos + n < pos) {
                throw TsCborException.truncated();
            }
        }

        long readHead(int wantMajor) {
            long[] mt = readAnyHead();
            if (mt[0] != wantMajor) {
                throw TsCborException.unexpected("major " + mt[0] + ", wanted " + wantMajor);
            }
            return mt[1];
        }

        // returns {major, argument}
        private long[] readAnyHead() {
            int ib = readByte();
            int major = ib >> 5;
            int info = ib & 0x1f;
            long arg;
            if (info < 24) {
                arg = info;
            } else if (info == 24) {
                arg = readByte();
            } else if (info == 25) {
                requireRemaining(2);
                arg = ((long) (buf[pos] & 0xff) << 8) | (buf[pos + 1] & 0xff);
                pos += 2;
            } else if (info == 26) {
                requireRemaining(4);
                arg = ((long) (buf[pos] & 0xff) << 24)
                        | ((long) (buf[pos + 1] & 0xff) << 16)
                        | ((long) (buf[pos + 2] & 0xff) << 8)
                        | (buf[pos + 3] & 0xff);
                pos += 4;
            } else if (info == 27) {
                requireRemaining(8);
                arg = 0;
                for (int i = 0; i < 8; i++) {
                    arg = (arg << 8) | (buf[pos + i] & 0xff);
                }
                pos += 8;
            } else {
                throw TsCborException.unexpected("reserved info " + info);
            }
            return new long[] {major, arg};
        }

        String readText() {
            int len = (int) readHead(MT_TEXT);
            requireRemaining(len);
            String s = new String(buf, pos, len, StandardCharsets.UTF_8);
            pos += len;
            return s;
        }

        long readInt() {
            long[] mt = readAnyHead();
            int major = (int) mt[0];
            long arg = mt[1];
            return switch (major) {
                case MT_UINT -> arg;
                case MT_NINT -> -1 - arg;
                default -> throw TsCborException.unexpected("int major " + major);
            };
        }

        double readF64() {
            int head = readByte();
            if (head != F64_HEAD) {
                throw TsCborException.unexpected("float head 0x" + Integer.toHexString(head));
            }
            requireRemaining(8);
            long bits = 0;
            for (int i = 0; i < 8; i++) {
                bits = (bits << 8) | (buf[pos + i] & 0xff);
            }
            pos += 8;
            return Double.longBitsToDouble(bits);
        }
    }
}
