package com.submillisecond.recipes.hll;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * HyperLogLog cardinality estimator.
 *
 * <p>Precision {@code p} in {@code [4, 18]}. Register array is {@code m = 2^p}
 * bytes. Hash splits into a {@code p}-bit register index (top bits) and a
 * leading-zero count on the remaining bits; the register stores the running
 * max. Estimate is {@code alpha * m^2 / sum(2^-r_i)}, with linear counting
 * at low cardinality.
 *
 * <p>Hash is FNV-1a 64-bit + SplitMix64 finalizer for clean bit distribution.
 *
 * <p><b>Thread safety.</b> Single-writer. {@code add}, {@code merge} and
 * {@code clear} mutate the register array with no synchronization, so two
 * threads calling {@code add} on one instance is a data race. The fan-in
 * pattern is a sketch per thread or shard and one {@code merge} at read time;
 * the merge is exact, so nothing is lost by never sharing a writer.
 * {@code estimate} is safe on an instance nobody is writing.
 */
public final class HyperLogLog {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    /** Lowest precision the estimator is calibrated for. */
    public static final int MIN_PRECISION = 4;
    /** Highest precision this recipe allocates for. 2^18 registers is 256 KB. */
    public static final int MAX_PRECISION = 18;
    /** Flajolet's asymptotic relative standard error constant. */
    public static final double RSE_CONSTANT = 1.04;

    // 2^-r for r in [0, 64]. `r` is leadingZeros(w) + 1 on a 64-bit word
    // with the high p bits zeroed, so r is bounded by 64-p+1 in practice.
    // estimate() calls Math.pow(2.0, -r) 16384 times for p=14 - JVM's
    // Math.pow is a generic-exponent intrinsic that's an order of
    // magnitude slower than this table read.
    private static final double[] INV_POW2 = new double[65];
    static {
        for (int r = 0; r < INV_POW2.length; r++) {
            INV_POW2[r] = Math.pow(2.0, -r);
        }
    }

    private final int p;
    private final int m;
    private final byte[] registers;
    private final double alpha;

    /**
     * New empty HLL. {@code precision} is clamped to {@code [4, 18]}; 14 gives
     * ~16k registers / ~16 KB / ~1% standard error. Use {@link #tryNew} when a
     * caller-supplied precision should be rejected rather than pulled into
     * range.
     */
    public HyperLogLog(int precision) {
        int pp = Math.min(MAX_PRECISION, Math.max(MIN_PRECISION, precision));
        this.p = pp;
        this.m = 1 << pp;
        this.registers = new byte[m];
        this.alpha = alphaM(m);
    }

    /**
     * New empty HLL, rejecting a precision outside {@code [4, 18]} instead of
     * clamping it. Reach for this when the precision comes from config or a
     * wire message and a typo should fail loudly.
     */
    public static HyperLogLog tryNew(int precision) {
        if (precision < MIN_PRECISION || precision > MAX_PRECISION) {
            throw HllException.invalidPrecision(precision);
        }
        return new HyperLogLog(precision);
    }

    public int precision() { return p; }
    public int registerCount() { return m; }

    /**
     * Analytic relative standard error, {@code 1.04 / sqrt(m)}. This is the
     * error the structure carries by construction, not a measurement of the
     * current contents: at p=14 it is 0.813%, so a 1,000,000 estimate is one
     * standard deviation away from anything in [992k, 1008k].
     */
    public double standardError() {
        return RSE_CONSTANT / Math.sqrt(m);
    }

    /**
     * Smallest precision whose standard error is at or below {@code target}
     * (a fraction, so 0.01 for 1%). Clamped to {@code [4, 18]}, so a target
     * finer than 0.26% returns 18 and the caller gets the best this recipe
     * allocates for rather than an error.
     */
    public static int precisionForStandardError(double target) {
        for (int p = MIN_PRECISION; p < MAX_PRECISION; p++) {
            if (RSE_CONSTANT / Math.sqrt(1 << p) <= target) {
                return p;
            }
        }
        return MAX_PRECISION;
    }

    /**
     * Bytes of register state. Fixed at construction and independent of how
     * many items the sketch has seen.
     */
    public int stateBytes() { return registers.length; }

    /** True while every register is still zero. */
    public boolean isEmpty() {
        for (byte r : registers) {
            if (r != 0) return false;
        }
        return true;
    }

    /**
     * Zero every register, keeping the allocation. Reuse across windows
     * without re-allocating the array.
     */
    public void clear() {
        Arrays.fill(registers, (byte) 0);
    }

    // Accessors for the feature modules (sparse promotion, union /
    // intersect, the wire codec), which live in the `features` sub-package and
    // so cannot see package-private members. Public for that reason only - NOT
    // part of the stable API surface; the arrays themselves stay private.
    public byte[] registers() { return registers; }
    public double alpha() { return alpha; }

    /**
     * Apply a sparse list of (registerIndex, rho) pairs into this HLL's
     * register array. Used by SparseHyperLogLog.promote().
     * Not part of the stable API surface.
     */
    public void applySparse(int[] indices, byte[] rhos) {
        int n = indices.length;
        for (int i = 0; i < n; i++) {
            int idx = indices[i];
            byte r = rhos[i];
            if (idx >= 0 && idx < registers.length && r > registers[idx]) {
                registers[idx] = r;
            }
        }
    }

    /**
     * Element-wise max of two equal-precision register arrays into this
     * HLL's registers. Used by UnionIntersect.estimateUnion().
     * Not part of the stable API surface.
     */
    public void applyPairedMax(byte[] a, byte[] b) {
        if (a.length != registers.length || b.length != registers.length) {
            throw new IllegalArgumentException("register length mismatch");
        }
        for (int i = 0; i < registers.length; i++) {
            byte x = a[i], y = b[i];
            byte mx = (x > y) ? x : y;
            if (mx > registers[i]) registers[i] = mx;
        }
    }

    /** Alpha lookup so feature modules don't re-implement it. Public for
     *  the `features` sub-package; not part of the stable API surface. */
    public static double alphaForRegisters(int m) {
        return alphaM(m);
    }

    /**
     * Record a key. Returns true when the sketch changed - a register moved
     * up, so this key was the first of its kind to land that deep. Matching
     * {@code PFADD}'s return, and cheap enough to ignore when you do not want
     * it.
     */
    public boolean add(String key) {
        return addBytes(key.getBytes(StandardCharsets.UTF_8));
    }

    /**
     * Record raw bytes. The string path funnels through here, so
     * {@code add("AAPL")} and {@code addBytes("AAPL".getBytes(UTF_8))} land in
     * the same register.
     */
    public boolean addBytes(byte[] key) {
        return addHash(mix(fnv1a64(key)));
    }

    /**
     * Record a 64-bit id without rendering it to a string first. Hashes the
     * big-endian bytes, so the Java and Rust ports agree register for register
     * on the same id.
     */
    public boolean addLong(long key) {
        byte[] buf = new byte[8];
        for (int i = 0; i < 8; i++) {
            buf[i] = (byte) (key >>> (56 - 8 * i));
        }
        return addBytes(buf);
    }

    private boolean addHash(long h) {
        int idx = (int) (h >>> (64 - p));
        long w = (h << p) | (1L << (p - 1));
        int r = Long.numberOfLeadingZeros(w) + 1;
        if (r > registers[idx]) {
            registers[idx] = (byte) r;
            return true;
        }
        return false;
    }

    public double estimate() {
        double sum = 0.0;
        int zeros = 0;
        final double[] inv = INV_POW2;
        for (byte b : registers) {
            int r = b & 0xff;
            if (r == 0) zeros++;
            sum += inv[r];
        }
        double raw = alpha * (double) m * (double) m / sum;
        if (zeros > 0 && raw <= 2.5 * m) {
            return -((double) m) * Math.log((double) zeros / (double) m);
        }
        return raw;
    }

    /** Merge another HLL of the same precision (element-wise max). */
    public void merge(HyperLogLog other) {
        if (this.p != other.p) {
            throw HllException.precisionMismatch(this.p, other.p);
        }
        for (int i = 0; i < m; i++) {
            if (other.registers[i] > this.registers[i]) {
                this.registers[i] = other.registers[i];
            }
        }
    }

    /** Independent copy, registers included. */
    public HyperLogLog copy() {
        HyperLogLog out = new HyperLogLog(p);
        System.arraycopy(registers, 0, out.registers, 0, m);
        return out;
    }

    /** Deliberately does not dump the register array - 16384 bytes into a log. */
    @Override
    public String toString() {
        return "HyperLogLog{p=" + p + ", m=" + m + ", estimate=" + estimate() + "}";
    }

    private static double alphaM(int m) {
        return switch (m) {
            case 16 -> 0.673;
            case 32 -> 0.697;
            case 64 -> 0.709;
            default -> 0.7213 / (1.0 + 1.079 / m);
        };
    }

    private static long fnv1a64(byte[] bytes) {
        long h = FNV_OFFSET;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }

    /** SplitMix64 finalizer. Fixes FNV-1a's poor bit distribution on short keys. */
    private static long mix(long h) {
        h ^= h >>> 30;
        h *= 0xbf58476d1ce4e5b9L;
        h ^= h >>> 27;
        h *= 0x94d049bb133111ebL;
        h ^= h >>> 31;
        return h;
    }
}
