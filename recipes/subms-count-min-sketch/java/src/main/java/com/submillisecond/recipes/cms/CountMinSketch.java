package com.submillisecond.recipes.cms;

/**
 * Count-Min Sketch with conservative update and Kirsch-Mitzenmacher hashing.
 *
 * <p>{@code d} rows of {@code w} counters; width is rounded up to a power of
 * two so indexing is a bitmask. Each insert finds the minimum cell across
 * the {@code d} rows and raises only the cells at that minimum. Query
 * returns the min cell.
 *
 * <p>Estimates are one-sided: {@code estimate(k) >= trueCount(k)} always,
 * with the over-count bounded by {@code relativeError() * total()} at
 * {@link #confidence()}.
 *
 * <p>Not thread-safe. A shared sketch needs external synchronisation; the
 * intended concurrent shape is one sketch per writer thread, folded with
 * {@code features.Merge} at the join.
 *
 * <p>The snapshot bytes produced by {@link #toBytes()} are byte-identical to
 * the Rust port's {@code to_bytes()}.
 */
public final class CountMinSketch {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    /**
     * Row cap. The add path keeps its index set in a fixed stack array, and
     * {@code d = 16} already puts the failure probability at {@code e^-16},
     * so a deeper sketch buys nothing a wider one would not buy more cheaply.
     */
    public static final int MAX_DEPTH = 16;

    private static final byte[] SNAPSHOT_MAGIC = {'S', 'U', 'B', 'M', 'S', 'C', 'M', 'S'};
    private static final int SNAPSHOT_VERSION = 1;
    private static final int SNAPSHOT_HEADER = 32;

    /** Why a byte array could not be decoded into a sketch. */
    public static final class SnapshotException extends RuntimeException {
        private static final long serialVersionUID = 1L;
        public SnapshotException(String msg) { super(msg); }
    }

    private final int d;
    private final int w;
    private final int mask;
    private final long seed;
    private final int[][] rows;
    private final int[] idxs;
    private long total;

    /**
     * {@code depth} hash functions (rows, clamped to {@code 2..MAX_DEPTH});
     * {@code width} is rounded up to a power of two. Standard sizing d=5,
     * w=16384 gives an additive error of at most {@code e/w} of the stream
     * volume with probability {@code 1 - e^-d}.
     */
    public CountMinSketch(int depth, int width) {
        this(depth, width, 0L);
    }

    /**
     * Same shape, with the hash family shifted by {@code seed}. Two sketches
     * only merge or compare if their seeds match.
     */
    public CountMinSketch(int depth, int width, long seed) {
        int dd = Math.min(MAX_DEPTH, Math.max(2, depth));
        int ww = Math.max(2, width);
        ww = Integer.highestOneBit(ww - 1) << 1; // round up to power of two
        this.d = dd;
        this.w = ww;
        this.mask = ww - 1;
        this.seed = seed;
        this.rows = new int[dd][ww];
        this.idxs = new int[dd];
    }

    /**
     * Size from the error budget instead of from {@code (depth, width)}.
     * {@code epsilon} is the tolerated over-count as a fraction of total
     * stream volume; {@code confidence} is the probability the bound holds.
     */
    public static CountMinSketch withErrorBounds(double epsilon, double confidence) {
        return withErrorBoundsSeeded(epsilon, confidence, 0L);
    }

    public static CountMinSketch withErrorBoundsSeeded(double epsilon, double confidence, long seed) {
        return new CountMinSketch(suggestDepth(confidence), suggestWidth(epsilon), seed);
    }

    /**
     * Width needed for an additive error of {@code epsilon * total}:
     * {@code ceil(e/epsilon)}, rounded up to a power of two.
     */
    public static int suggestWidth(double epsilon) {
        if (Double.isNaN(epsilon) || epsilon <= 0.0) return 1 << 30;
        double raw = Math.ceil(Math.E / epsilon);
        if (raw >= (double) (1 << 30)) return 1 << 30;
        int ww = Math.max(2, (int) raw);
        return Integer.highestOneBit(ww - 1) << 1;
    }

    /**
     * Depth needed for the error bound to hold with probability
     * {@code confidence}: {@code ceil(ln(1/(1-confidence)))}, clamped to
     * {@code 2..MAX_DEPTH}.
     */
    public static int suggestDepth(double confidence) {
        if (Double.isNaN(confidence) || confidence <= 0.0) return 2;
        if (confidence >= 1.0) return MAX_DEPTH;
        double raw = Math.ceil(Math.log(1.0 / (1.0 - confidence)));
        if (raw >= MAX_DEPTH) return MAX_DEPTH;
        return Math.min(MAX_DEPTH, Math.max(2, (int) raw));
    }

    public int depth() { return d; }
    public int width() { return w; }
    public long seed() { return seed; }

    /**
     * Total weight ingested, exactly. Unlike the per-key estimates this is a
     * running sum, not a sketch, so it carries no error.
     */
    public long total() { return total; }

    public boolean isEmpty() { return total == 0L; }

    /** Additive error as a fraction of total volume: {@code e / w}. */
    public double relativeError() { return Math.E / (double) w; }

    /** Probability the error bound holds: {@code 1 - e^-d}. */
    public double confidence() { return 1.0 - Math.exp(-(double) d); }

    /** Absolute over-count budget at the current volume: {@code ceil(e/w * total)}. */
    public int errorMargin() {
        double m = Math.ceil(relativeError() * (double) total);
        return m >= (double) Integer.MAX_VALUE ? Integer.MAX_VALUE : (int) m;
    }

    /**
     * Fraction of cells that have ever been touched. Climbing past ~0.5 means
     * the sketch is undersized for the key cardinality it is seeing.
     * O(d*w) - a monitoring call, not a hot-path one.
     */
    public double occupancy() {
        long used = 0;
        for (int i = 0; i < d; i++) {
            int[] row = rows[i];
            for (int j = 0; j < w; j++) if (row[j] != 0) used++;
        }
        return (double) used / (double) ((long) d * w);
    }

    /**
     * Counter-matrix footprint in bytes. Fixed at construction: the sketch
     * never grows with key cardinality, which is the whole reason to use one.
     */
    public long heapBytes() { return (long) d * w * Integer.BYTES; }

    /** Increment the count of {@code key} by 1. */
    public void add(String key) {
        bump(baseHash(fnv1a64Utf8(key)), 1);
    }

    /**
     * Increment the count of {@code key} by {@code n}. A weighted update -
     * notional, message bytes, filled quantity - not just an occurrence count.
     *
     * @throws IllegalArgumentException when {@code n} is negative; the sketch
     *         has no decrement path and a silent no-op would hide the bug.
     */
    public void addN(String key, int n) {
        requireNonNegative(n);
        bump(baseHash(fnv1a64Utf8(key)), n);
    }

    public void addBytes(byte[] key) {
        bump(baseHash(fnv1a64(key)), 1);
    }

    public void addBytesN(byte[] key, int n) {
        requireNonNegative(n);
        bump(baseHash(fnv1a64(key)), n);
    }

    /**
     * Increment an integer key by 1. Identical to hashing the key's
     * little-endian bytes, without materialising them.
     */
    public void addU64(long key) {
        bump(baseHash(fnv1a64(key)), 1);
    }

    public void addU64N(long key, int n) {
        requireNonNegative(n);
        bump(baseHash(fnv1a64(key)), n);
    }

    /**
     * Estimated count for {@code key}. Always {@code >=} the true count; the
     * over-count is bounded by {@link #errorMargin()}.
     */
    public int estimate(String key) {
        return minCell(baseHash(fnv1a64Utf8(key)));
    }

    public int estimateBytes(byte[] key) {
        return minCell(baseHash(fnv1a64(key)));
    }

    public int estimateU64(long key) {
        return minCell(baseHash(fnv1a64(key)));
    }

    /**
     * The other end of the interval: {@code estimate - errorMargin}, floored
     * at zero. The true count lies in {@code [lowerBound, estimate]} at
     * {@link #confidence()}.
     */
    public int estimateLowerBound(String key) {
        return Math.max(0, estimate(key) - errorMargin());
    }

    /**
     * Zero every counter and reset the volume. Shape and seed are kept, so a
     * long-lived sketch can be recycled without reallocating the matrix.
     */
    public void clear() {
        for (int i = 0; i < d; i++) {
            java.util.Arrays.fill(rows[i], 0);
        }
        total = 0L;
    }

    /**
     * Snapshot to a self-describing byte array: a 32-byte header (magic,
     * version, depth, width, seed, total) then {@code d * w} little-endian
     * {@code int} counters, row-major.
     */
    public byte[] toBytes() {
        byte[] out = new byte[SNAPSHOT_HEADER + d * w * 4];
        System.arraycopy(SNAPSHOT_MAGIC, 0, out, 0, 8);
        putShort(out, 8, SNAPSHOT_VERSION);
        putShort(out, 10, d);
        putInt(out, 12, w);
        putLong(out, 16, seed);
        putLong(out, 24, total);
        int at = SNAPSHOT_HEADER;
        for (int i = 0; i < d; i++) {
            int[] row = rows[i];
            for (int j = 0; j < w; j++) {
                putInt(out, at, row[j]);
                at += 4;
            }
        }
        return out;
    }

    /**
     * Inverse of {@link #toBytes()}. Rejects a foreign or truncated buffer
     * rather than decoding a plausible-looking sketch out of it.
     *
     * @throws SnapshotException on bad magic, an unknown version, an
     *         impossible shape, or a length that does not match the header.
     */
    public static CountMinSketch fromBytes(byte[] bytes) {
        if (bytes.length < SNAPSHOT_HEADER) {
            throw new SnapshotException(
                "truncated snapshot: expected " + SNAPSHOT_HEADER + " bytes, got " + bytes.length);
        }
        for (int i = 0; i < 8; i++) {
            if (bytes[i] != SNAPSHOT_MAGIC[i]) {
                throw new SnapshotException("not a count-min-sketch snapshot");
            }
        }
        int version = getShort(bytes, 8);
        if (version != SNAPSHOT_VERSION) {
            throw new SnapshotException("unsupported snapshot version " + version);
        }
        int depth = getShort(bytes, 10);
        int width = getInt(bytes, 12);
        if (depth < 2 || depth > MAX_DEPTH || width < 2 || Integer.bitCount(width) != 1) {
            throw new SnapshotException("invalid shape: depth=" + depth + ", width=" + width);
        }
        int expected = SNAPSHOT_HEADER + depth * width * 4;
        if (bytes.length != expected) {
            throw new SnapshotException(
                "truncated snapshot: expected " + expected + " bytes, got " + bytes.length);
        }
        CountMinSketch sketch = new CountMinSketch(depth, width, getLong(bytes, 16));
        int at = SNAPSHOT_HEADER;
        for (int i = 0; i < depth; i++) {
            int[] row = sketch.rows[i];
            for (int j = 0; j < width; j++) {
                row[j] = getInt(bytes, at);
                at += 4;
            }
        }
        sketch.total = getLong(bytes, 24);
        return sketch;
    }

    @Override
    public String toString() {
        return "CountMinSketch{depth=" + d + ", width=" + w + ", seed=" + seed + ", total=" + total + "}";
    }

    /**
     * Internal: in-place element-wise fold from {@code other}, summing when
     * {@code sum} is set and taking the maximum otherwise. Public so the
     * {@code features.Merge} class in the sub-package can call it. Caller
     * validates shape and seed.
     *
     * @apiNote not part of the stable API surface.
     */
    public void applyPaired(CountMinSketch other, boolean sum) {
        for (int i = 0; i < d; i++) {
            int[] dst = rows[i];
            int[] src = other.rows[i];
            for (int j = 0; j < w; j++) {
                dst[j] = sum ? saturatingAdd(dst[j], src[j]) : Math.max(dst[j], src[j]);
            }
        }
        total = saturatingAddLong(total, other.total);
    }

    private static void requireNonNegative(int n) {
        if (n < 0) throw new IllegalArgumentException("negative weight: " + n);
    }

    /**
     * Conservative update: raise each of the {@code d} cells to {@code min+n}
     * and leave any cell already above that alone. The min-query never reads
     * those higher cells for this key, so raising them would only add slop for
     * whatever else collides there.
     */
    private void bump(long packedH1H2, int n) {
        if (n == 0) return;
        long h1 = packedH1H2 & 0xffffffffL;
        long h2 = (packedH1H2 >>> 32) | 1L;
        int min = Integer.MAX_VALUE;
        for (int i = 0; i < d; i++) {
            int idx = (int) (h1 + (long) i * h2) & mask;
            idxs[i] = idx;
            if (rows[i][idx] < min) min = rows[i][idx];
        }
        int floor = saturatingAdd(min, n);
        for (int i = 0; i < d; i++) {
            if (rows[i][idxs[i]] < floor) rows[i][idxs[i]] = floor;
        }
        total = saturatingAddLong(total, n);
    }

    private int minCell(long packedH1H2) {
        long h1 = packedH1H2 & 0xffffffffL;
        long h2 = (packedH1H2 >>> 32) | 1L;
        int min = Integer.MAX_VALUE;
        for (int i = 0; i < d; i++) {
            int idx = (int) (h1 + (long) i * h2) & mask;
            if (rows[i][idx] < min) min = rows[i][idx];
        }
        return min;
    }

    private static int saturatingAdd(int a, int b) {
        int sum = a + b;
        return ((a ^ sum) & (b ^ sum)) < 0 ? Integer.MAX_VALUE : sum;
    }

    private static long saturatingAddLong(long a, long b) {
        long sum = a + b;
        return ((a ^ sum) & (b ^ sum)) < 0 ? Long.MAX_VALUE : sum;
    }

    /**
     * The two base hashes packed into one long: h1 in the low 32 bits, h2 in
     * the high 32. Returning a primitive keeps the hot path allocation-free.
     */
    private long baseHash(long fnv) {
        return mix(fnv ^ seed);
    }

    private static long fnv1a64(byte[] bytes) {
        long h = FNV_OFFSET;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }

    /** FNV-1a over the key's little-endian bytes, without building the array. */
    private static long fnv1a64(long key) {
        long h = FNV_OFFSET;
        for (int i = 0; i < 8; i++) {
            h ^= (key >>> (8 * i)) & 0xffL;
            h *= FNV_PRIME;
        }
        return h;
    }

    /**
     * FNV-1a over the UTF-8 encoding of {@code s}, encoded on the fly.
     * {@code String.getBytes(UTF_8)} would allocate a fresh array on every add,
     * and this is the add path. Byte-for-byte equal to hashing that array,
     * including the '?' substitution the JDK applies to an unpaired surrogate.
     */
    private static long fnv1a64Utf8(String s) {
        long h = FNV_OFFSET;
        int n = s.length();
        int i = 0;
        while (i < n) {
            char c = s.charAt(i++);
            if (c < 0x80) {
                h = fnvByte(h, c);
            } else if (c < 0x800) {
                h = fnvByte(h, 0xC0 | (c >> 6));
                h = fnvByte(h, 0x80 | (c & 0x3F));
            } else if (Character.isHighSurrogate(c) && i < n && Character.isLowSurrogate(s.charAt(i))) {
                int cp = Character.toCodePoint(c, s.charAt(i++));
                h = fnvByte(h, 0xF0 | (cp >> 18));
                h = fnvByte(h, 0x80 | ((cp >> 12) & 0x3F));
                h = fnvByte(h, 0x80 | ((cp >> 6) & 0x3F));
                h = fnvByte(h, 0x80 | (cp & 0x3F));
            } else if (Character.isSurrogate(c)) {
                h = fnvByte(h, '?');
            } else {
                h = fnvByte(h, 0xE0 | (c >> 12));
                h = fnvByte(h, 0x80 | ((c >> 6) & 0x3F));
                h = fnvByte(h, 0x80 | (c & 0x3F));
            }
        }
        return h;
    }

    private static long fnvByte(long h, int b) {
        return (h ^ (b & 0xffL)) * FNV_PRIME;
    }

    private static long mix(long h) {
        h ^= h >>> 30;
        h *= 0xbf58476d1ce4e5b9L;
        h ^= h >>> 27;
        h *= 0x94d049bb133111ebL;
        h ^= h >>> 31;
        return h;
    }

    private static void putShort(byte[] out, int at, int v) {
        out[at] = (byte) v;
        out[at + 1] = (byte) (v >>> 8);
    }

    private static void putInt(byte[] out, int at, int v) {
        out[at] = (byte) v;
        out[at + 1] = (byte) (v >>> 8);
        out[at + 2] = (byte) (v >>> 16);
        out[at + 3] = (byte) (v >>> 24);
    }

    private static void putLong(byte[] out, int at, long v) {
        for (int i = 0; i < 8; i++) out[at + i] = (byte) (v >>> (8 * i));
    }

    private static int getShort(byte[] in, int at) {
        return (in[at] & 0xff) | ((in[at + 1] & 0xff) << 8);
    }

    private static int getInt(byte[] in, int at) {
        return (in[at] & 0xff)
            | ((in[at + 1] & 0xff) << 8)
            | ((in[at + 2] & 0xff) << 16)
            | ((in[at + 3] & 0xff) << 24);
    }

    private static long getLong(byte[] in, int at) {
        long v = 0;
        for (int i = 0; i < 8; i++) v |= (in[at + i] & 0xffL) << (8 * i);
        return v;
    }
}
