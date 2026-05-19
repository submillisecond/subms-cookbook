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
        for (byte b : registers) {
            int r = b & 0xff;
            if (r == 0) zeros++;
            sum += Math.pow(2.0, -r);
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
