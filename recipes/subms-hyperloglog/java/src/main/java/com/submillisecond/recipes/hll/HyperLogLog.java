package com.submillisecond.recipes.hll;

import java.nio.charset.StandardCharsets;

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
 */
public final class HyperLogLog {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

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

    public HyperLogLog(int precision) {
        int pp = Math.min(18, Math.max(4, precision));
        this.p = pp;
        this.m = 1 << pp;
        this.registers = new byte[m];
        this.alpha = alphaM(m);
    }

    public int precision() { return p; }
    public int registerCount() { return m; }

    // Accessors for the feature modules (sparse promotion, union /
    // intersect), which live in the `features` sub-package and so cannot
    // see package-private members. Public for that reason only - NOT part
    // of the stable API surface; the arrays themselves stay private.
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

    public void add(String key) {
        long h = mix(fnv1a64(key.getBytes(StandardCharsets.UTF_8)));
        int idx = (int) (h >>> (64 - p));
        long w = (h << p) | (1L << (p - 1));
        int r = Long.numberOfLeadingZeros(w) + 1;
        if (r > registers[idx]) registers[idx] = (byte) r;
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
        if (this.p != other.p) throw new IllegalArgumentException("precision mismatch");
        for (int i = 0; i < m; i++) {
            if (other.registers[i] > this.registers[i]) {
                this.registers[i] = other.registers[i];
            }
        }
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
