package com.submillisecond.recipes.hll;

import com.submillisecond.recipes.hll.features.SparseHyperLogLog;

/**
 * Canonical wire format. A sketch is worth ~1% of the bytes the raw ids would
 * cost, which only pays off if it can leave the process: checkpoint to disk,
 * ship a per-shard partial to a collector, cache a per-key sketch in Redis.
 * That needs a format both ports agree on byte for byte, and both ports emit
 * exactly the bytes below.
 *
 * <pre>
 * 0..4   magic  "SHLL"
 * 4      format version (1)
 * 5      encoding: 0 dense, 1 sparse
 * 6      precision p
 * 7      reserved, zero
 * dense  8..8+m           m register bytes, one per register
 * sparse 8..12            u32 BE promotion threshold
 *        12..16           u32 BE entry count n
 *        16..16+5n        n * (u32 BE register index, u8 rho)
 * </pre>
 *
 * <p>Multi-byte fields are big-endian, so a hex dump reads left to right and
 * the Rust port needs no byte-order argument either.
 *
 * <p>This is the recipe's own format. It is not Redis's {@code PFADD} string
 * and it is not a DataSketches {@code HllSketch} image; neither will read
 * these bytes.
 */
public final class HllCodec {

    /** Leading bytes of every buffer this codec writes. */
    public static final byte[] MAGIC = {'S', 'H', 'L', 'L'};
    /** Format version. Bumped only on a breaking layout change. */
    public static final byte FORMAT_VERSION = 1;

    static final byte ENC_DENSE = 0;
    static final byte ENC_SPARSE = 1;
    static final int HEADER_LEN = 8;

    private HllCodec() {}

    /**
     * Serialise to the canonical dense form: an 8-byte header then the raw
     * register array. Length is always {@code 8 + 2^p}, so a reader can size
     * the allocation from the header alone.
     */
    public static byte[] toBytes(HyperLogLog hll) {
        byte[] regs = hll.registers();
        byte[] out = new byte[HEADER_LEN + regs.length];
        writeHeader(out, ENC_DENSE, hll.precision());
        System.arraycopy(regs, 0, out, HEADER_LEN, regs.length);
        return out;
    }

    /**
     * Parse a dense buffer. A sparse buffer is rejected with
     * {@code UNSUPPORTED_ENCODING} rather than silently densified - use
     * {@link #sparseFromBytes}, which reads both.
     */
    public static HyperLogLog fromBytes(byte[] bytes) {
        int[] header = readHeader(bytes);
        if (header[0] != ENC_DENSE) {
            throw HllException.unsupportedEncoding(header[0]);
        }
        int p = header[1];
        int m = 1 << p;
        int expected = HEADER_LEN + m;
        if (bytes.length < expected) {
            throw HllException.truncated(expected, bytes.length);
        }
        HyperLogLog out = new HyperLogLog(p);
        System.arraycopy(bytes, HEADER_LEN, out.registers(), 0, m);
        return out;
    }

    /**
     * Serialise in whichever representation the sketch currently holds. A thin
     * sketch stays thin on the wire; a promoted one writes the same dense
     * buffer {@link #toBytes(HyperLogLog)} would.
     */
    public static byte[] toBytes(SparseHyperLogLog sparse) {
        HyperLogLog dense = sparse.asDense();
        if (dense != null) {
            return toBytes(dense);
        }
        int n = sparse.entryCount();
        byte[] out = new byte[HEADER_LEN + 8 + n * 5];
        writeHeader(out, ENC_SPARSE, sparse.precision());
        writeInt(out, HEADER_LEN, sparse.threshold());
        writeInt(out, HEADER_LEN + 4, n);
        int[] idx = sparse.entryIndices();
        byte[] rho = sparse.entryRhos();
        for (int i = 0; i < n; i++) {
            int at = HEADER_LEN + 8 + i * 5;
            writeInt(out, at, idx[i]);
            out[at + 4] = rho[i];
        }
        return out;
    }

    /**
     * Parse either encoding. A dense buffer comes back as an already-promoted
     * sketch, which is the honest reading: the writer had crossed the
     * threshold and the reader inherits that.
     */
    public static SparseHyperLogLog sparseFromBytes(byte[] bytes) {
        int[] header = readHeader(bytes);
        if (header[0] == ENC_DENSE) {
            return SparseHyperLogLog.fromDense(fromBytes(bytes));
        }
        if (header[0] != ENC_SPARSE) {
            throw HllException.unsupportedEncoding(header[0]);
        }
        if (bytes.length < HEADER_LEN + 8) {
            throw HllException.truncated(HEADER_LEN + 8, bytes.length);
        }
        int threshold = readInt(bytes, HEADER_LEN);
        int count = readInt(bytes, HEADER_LEN + 4);
        int expected = HEADER_LEN + 8 + count * 5;
        if (bytes.length < expected) {
            throw HllException.truncated(expected, bytes.length);
        }
        int[] idx = new int[count];
        byte[] rho = new byte[count];
        for (int i = 0; i < count; i++) {
            int at = HEADER_LEN + 8 + i * 5;
            idx[i] = readInt(bytes, at);
            rho[i] = bytes[at + 4];
        }
        return SparseHyperLogLog.fromEntries(header[1], threshold, idx, rho);
    }

    private static void writeHeader(byte[] out, byte encoding, int p) {
        System.arraycopy(MAGIC, 0, out, 0, MAGIC.length);
        out[4] = FORMAT_VERSION;
        out[5] = encoding;
        out[6] = (byte) p;
        out[7] = 0;
    }

    /** Returns {@code {encoding, precision}}. */
    private static int[] readHeader(byte[] bytes) {
        if (bytes.length < HEADER_LEN) {
            throw HllException.truncated(HEADER_LEN, bytes.length);
        }
        for (int i = 0; i < MAGIC.length; i++) {
            if (bytes[i] != MAGIC[i]) {
                throw HllException.badMagic();
            }
        }
        if (bytes[4] != FORMAT_VERSION) {
            throw HllException.unsupportedVersion(bytes[4]);
        }
        int p = bytes[6] & 0xff;
        if (p < HyperLogLog.MIN_PRECISION || p > HyperLogLog.MAX_PRECISION) {
            throw HllException.invalidPrecision(p);
        }
        return new int[] {bytes[5], p};
    }

    private static void writeInt(byte[] out, int at, int v) {
        out[at]     = (byte) (v >>> 24);
        out[at + 1] = (byte) (v >>> 16);
        out[at + 2] = (byte) (v >>> 8);
        out[at + 3] = (byte) v;
    }

    private static int readInt(byte[] b, int at) {
        return ((b[at] & 0xff) << 24) | ((b[at + 1] & 0xff) << 16)
             | ((b[at + 2] & 0xff) << 8) | (b[at + 3] & 0xff);
    }
}
