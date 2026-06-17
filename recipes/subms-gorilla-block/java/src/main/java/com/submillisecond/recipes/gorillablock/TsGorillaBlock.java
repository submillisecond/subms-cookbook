package com.submillisecond.recipes.gorillablock;

import java.io.IOException;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import com.submillisecond.recipes.ts.TsPoint;

/**
 * A Gorilla-compressed run of {@code (long ts, double value)} points
 * (Pelkonen et al., VLDB 2015): delta-of-delta timestamps + XOR-delta f64
 * values, ~1.5 bytes per point versus 16 raw. Append-only; timestamps must be
 * non-decreasing. Decode is whole-block (the Gorilla stream is not
 * random-access), which suits the cold-tier scan pattern.
 *
 * <p>The wire format is byte-equivalent to the Rust {@code subms-gorilla-block}
 * crate: a block encoded in one decodes byte-for-byte in the other.
 */
public final class TsGorillaBlock implements Iterable<TsPoint<Double>> {

    private static final int VERSION = 1;
    private static final int NO_WINDOW = -1;

    private BitWriter writer;
    private int count;
    private long firstTs;
    private long lastTs;
    private long prevDelta;
    private long prevValue;
    private int prevLeading;
    private int prevTrailing;
    private double valMin;
    private double valMax;

    public TsGorillaBlock() {
        this(64);
    }

    public TsGorillaBlock(int byteCap) {
        this.writer = new BitWriter(byteCap);
        this.count = 0;
        this.firstTs = 0;
        this.lastTs = 0;
        this.prevDelta = 0;
        this.prevValue = 0;
        this.prevLeading = NO_WINDOW;
        this.prevTrailing = 0;
        this.valMin = Double.POSITIVE_INFINITY;
        this.valMax = Double.NEGATIVE_INFINITY;
    }

    public static TsGorillaBlock withCapacity(int byteCap) {
        return new TsGorillaBlock(byteCap);
    }

    public int len() {
        return count;
    }

    public boolean isEmpty() {
        return count == 0;
    }

    /** Append a point. Timestamps must be non-decreasing across calls. */
    public void append(long ts, double value) {
        long vbits = Double.doubleToRawLongBits(value);
        if (count == 0) {
            writer.writeBits(ts, 64);
            writer.writeBits(vbits, 64);
            firstTs = ts;
            lastTs = ts;
            prevValue = vbits;
        } else {
            long delta = ts - lastTs;
            long dod = delta - prevDelta;
            encodeDod(writer, dod);
            lastTs = ts;
            prevDelta = delta;
            encodeValue(vbits);
        }
        if (value < valMin) {
            valMin = value;
        }
        if (value > valMax) {
            valMax = value;
        }
        count++;
    }

    /** Versioned wire bytes: {@code [version u8][count u32 LE][bitstream]}. */
    public byte[] bytes() {
        byte[] stream = count > 0 ? writer.snapshot() : new byte[0];
        byte[] out = new byte[5 + stream.length];
        out[0] = (byte) VERSION;
        out[1] = (byte) (count & 0xff);
        out[2] = (byte) ((count >>> 8) & 0xff);
        out[3] = (byte) ((count >>> 16) & 0xff);
        out[4] = (byte) ((count >>> 24) & 0xff);
        System.arraycopy(stream, 0, out, 5, stream.length);
        return out;
    }

    public static TsGorillaBlock fromBytes(byte[] bytes) {
        List<TsPoint<Double>> points = decodeAll(bytes);
        TsGorillaBlock b = withCapacity(bytes.length);
        for (TsPoint<Double> p : points) {
            b.append(p.ts(), p.value());
        }
        return b;
    }

    /**
     * Decode bytes straight to points without rebuilding an appendable block.
     * The cold-tier scan path: cheaper than {@link #fromBytes} when you only
     * read.
     */
    public static List<TsPoint<Double>> decode(byte[] bytes) {
        return decodeAll(bytes);
    }

    /**
     * Decode a block file via a read-only memory map. {@link FileChannel#map}
     * backs the read with the page cache rather than a buffered input stream,
     * so sweeping a large file of many blocks only faults the pages a query
     * touches. The Gorilla stream is whole-block decode, so a single block is
     * read in full regardless; the mmap win is the cold-tier sweep, not
     * per-block. This mirrors the Rust {@code TsMmapBlock}; the Java decode is
     * {@code byte[]}-based, so the mapped region is read into the decoder.
     */
    public static List<TsPoint<Double>> decodeMmap(Path path) throws IOException {
        try (FileChannel ch = FileChannel.open(path, StandardOpenOption.READ)) {
            int size = Math.toIntExact(ch.size());
            MappedByteBuffer mb = ch.map(FileChannel.MapMode.READ_ONLY, 0, size);
            byte[] bytes = new byte[size];
            mb.get(bytes);
            return decodeAll(bytes);
        }
    }

    /** Map a block file and rebuild an appendable block from it. */
    public static TsGorillaBlock fromMmap(Path path) throws IOException {
        List<TsPoint<Double>> points = decodeMmap(path);
        TsGorillaBlock b = withCapacity(points.size() * 2 + 16);
        for (TsPoint<Double> p : points) {
            b.append(p.ts(), p.value());
        }
        return b;
    }

    @Override
    public Iterator<TsPoint<Double>> iterator() {
        return decodeAll(bytes()).iterator();
    }

    /** Inclusive {@code [lo, hi]} filter over the decoded points. */
    public List<TsPoint<Double>> range(long lo, long hi) {
        List<TsPoint<Double>> out = new ArrayList<>();
        for (TsPoint<Double> p : decodeAll(bytes())) {
            if (p.ts() >= lo && p.ts() <= hi) {
                out.add(p);
            }
        }
        return out;
    }

    /**
     * Concatenate two blocks. Points merge in non-decreasing ts order, so a
     * block sealed earlier can fold into a later one.
     */
    public TsGorillaBlock merge(TsGorillaBlock other) {
        List<TsPoint<Double>> all = new ArrayList<>(decodeAll(bytes()));
        all.addAll(decodeAll(other.bytes()));
        all.sort((a, b) -> Long.compare(a.ts(), b.ts()));
        TsGorillaBlock b = withCapacity(bytes().length + other.bytes().length);
        for (TsPoint<Double> p : all) {
            b.append(p.ts(), p.value());
        }
        return b;
    }

    public TsBlockStats stats() {
        return new TsBlockStats(
                count,
                firstTs,
                lastTs,
                count == 0 ? 0.0 : valMin,
                count == 0 ? 0.0 : valMax);
    }

    private static void encodeDod(BitWriter w, long dod) {
        if (dod == 0) {
            w.writeBit(0);
        } else if (dod >= -64 && dod <= 63) {
            w.writeBits(0b10, 2);
            w.writeBits(dod & 0x7F, 7);
        } else if (dod >= -256 && dod <= 255) {
            w.writeBits(0b110, 3);
            w.writeBits(dod & 0x1FF, 9);
        } else if (dod >= -2048 && dod <= 2047) {
            w.writeBits(0b1110, 4);
            w.writeBits(dod & 0xFFF, 12);
        } else {
            w.writeBits(0b1111, 4);
            w.writeBits(dod, 64);
        }
    }

    private void encodeValue(long vbits) {
        long xor = vbits ^ prevValue;
        if (xor == 0) {
            writer.writeBit(0);
        } else {
            writer.writeBit(1);
            int leading = Math.min(Long.numberOfLeadingZeros(xor), 31);
            int trailing = Long.numberOfTrailingZeros(xor);
            if (prevLeading != NO_WINDOW && leading >= prevLeading && trailing >= prevTrailing) {
                writer.writeBit(0);
                int mlen = 64 - prevLeading - prevTrailing;
                writer.writeBits(xor >>> prevTrailing, mlen);
            } else {
                writer.writeBit(1);
                writer.writeBits(leading, 5);
                int mlen = 64 - leading - trailing;
                // mlen is 1..=64; 64 stored as 0 in the 6-bit field.
                writer.writeBits(mlen & 0x3F, 6);
                writer.writeBits(xor >>> trailing, mlen);
                prevLeading = leading;
                prevTrailing = trailing;
            }
        }
        prevValue = vbits;
    }

    private static long signExtend(long v, int n) {
        int shift = 64 - n;
        return (v << shift) >> shift;
    }

    private static long decodeDod(BitReader r) {
        int b0 = r.readBit();
        if (b0 < 0) {
            throw TsBlockException.truncated();
        }
        if (b0 == 0) {
            return 0;
        }
        int n;
        if (readBitOrThrow(r) == 0) {
            n = 7;
        } else if (readBitOrThrow(r) == 0) {
            n = 9;
        } else if (readBitOrThrow(r) == 0) {
            n = 12;
        } else {
            n = 64;
        }
        long raw = r.readBits(n);
        return signExtend(raw, n);
    }

    private static List<TsPoint<Double>> decodeAll(byte[] bytes) {
        if (bytes.length == 0) {
            return new ArrayList<>();
        }
        if ((bytes[0] & 0xff) != VERSION) {
            throw TsBlockException.badVersion(bytes[0] & 0xff);
        }
        if (bytes.length < 5) {
            throw TsBlockException.truncated();
        }
        long count = (bytes[1] & 0xffL)
                | ((bytes[2] & 0xffL) << 8)
                | ((bytes[3] & 0xffL) << 16)
                | ((bytes[4] & 0xffL) << 24);
        if (count == 0) {
            return new ArrayList<>();
        }
        byte[] stream = new byte[bytes.length - 5];
        System.arraycopy(bytes, 5, stream, 0, stream.length);
        BitReader r = new BitReader(stream);
        long ts0 = r.readBits(64);
        long v0 = r.readBits(64);
        List<TsPoint<Double>> out = new ArrayList<>((int) count);
        out.add(new TsPoint<>(ts0, Double.longBitsToDouble(v0)));

        long lastTs = ts0;
        long prevDelta = 0;
        long prevValue = v0;
        int leading = 0;
        int trailing = 0;

        for (long i = 1; i < count; i++) {
            long dod = decodeDod(r);
            long delta = prevDelta + dod;
            long ts = lastTs + delta;
            lastTs = ts;
            prevDelta = delta;

            if (readBitOrThrow(r) == 1) {
                if (readBitOrThrow(r) == 0) {
                    int mlen = 64 - leading - trailing;
                    long mant = r.readBits(mlen);
                    prevValue ^= mant << trailing;
                } else {
                    leading = (int) r.readBits(5);
                    int mlen = (int) r.readBits(6);
                    if (mlen == 0) {
                        mlen = 64;
                    }
                    trailing = 64 - leading - mlen;
                    long mant = r.readBits(mlen);
                    prevValue ^= mant << trailing;
                }
            }
            out.add(new TsPoint<>(ts, Double.longBitsToDouble(prevValue)));
        }
        return out;
    }

    private static int readBitOrThrow(BitReader r) {
        int b = r.readBit();
        if (b < 0) {
            throw TsBlockException.truncated();
        }
        return b;
    }
}
