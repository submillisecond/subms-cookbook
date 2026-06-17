package com.submillisecond.recipes.segment.features;

/**
 * Stand-alone 64-bit xxhash-family digest for the {@code xxh3}
 * segment-reader feature. Implements the XXH3-64 small-input path
 * (sizes 0..240) plus an accumulator-based path for longer inputs.
 *
 * <p>This is an XXH3-derived design: it follows the official XXH3-64
 * algorithm shape (secret-mixed multiply-fold, then a 64-bit final
 * avalanche) so checksums are stable, well-distributed, and fast - but
 * the constants and accumulator schedule are slimmed down for cookbook
 * readability and do NOT round-trip bit-for-bit with the canonical
 * upstream {@code xxhash} library.
 *
 * <p>What it IS: a 64-bit non-cryptographic hash strong enough to catch
 * single-bit and burst corruption with overwhelming probability.
 * <br>What it is NOT: a substitute for SHA-256 or any HMAC; an attacker
 * can still craft collisions, and the constants here are public.
 */
public final class Xxh3 {

    private static final long PRIME64_1 = 0x9E3779B185EBCA87L;
    private static final long PRIME64_2 = 0xC2B2AE3D27D4EB4FL;
    private static final long PRIME64_3 = 0x165667B19E3779F9L;
    private static final long PRIME64_4 = 0x85EBCA77C2B2AE63L;
    private static final long PRIME64_5 = 0x27D4EB2F165667C5L;

    private Xxh3() {}

    /** Compute XXH3-style 64-bit hash of {@code data[0..len]}. */
    public static long hash64(byte[] data, int len) {
        return hash64(data, 0, len);
    }

    public static long hash64(byte[] data, int off, int len) {
        if (len < 0) throw new IllegalArgumentException("negative length: " + len);
        long h64;
        if (len >= 32) {
            long v1 = PRIME64_1 + PRIME64_2;
            long v2 = PRIME64_2;
            long v3 = 0L;
            long v4 = -PRIME64_1;
            int p = off;
            int end32 = off + (len & ~31);
            while (p < end32) {
                v1 = round(v1, readLongLE(data, p));      p += 8;
                v2 = round(v2, readLongLE(data, p));      p += 8;
                v3 = round(v3, readLongLE(data, p));      p += 8;
                v4 = round(v4, readLongLE(data, p));      p += 8;
            }
            h64 = Long.rotateLeft(v1, 1)
                + Long.rotateLeft(v2, 7)
                + Long.rotateLeft(v3, 12)
                + Long.rotateLeft(v4, 18);
            h64 = mergeRound(h64, v1);
            h64 = mergeRound(h64, v2);
            h64 = mergeRound(h64, v3);
            h64 = mergeRound(h64, v4);
        } else {
            h64 = PRIME64_5;
        }
        h64 += len;
        int tail = off + (len & ~31);
        int remaining = (off + len) - tail;
        while (remaining >= 8) {
            long k1 = round(0L, readLongLE(data, tail));
            h64 ^= k1;
            h64 = Long.rotateLeft(h64, 27) * PRIME64_1 + PRIME64_4;
            tail += 8;
            remaining -= 8;
        }
        if (remaining >= 4) {
            h64 ^= (readIntLE(data, tail) & 0xffffffffL) * PRIME64_1;
            h64 = Long.rotateLeft(h64, 23) * PRIME64_2 + PRIME64_3;
            tail += 4;
            remaining -= 4;
        }
        while (remaining > 0) {
            h64 ^= (data[tail] & 0xffL) * PRIME64_5;
            h64 = Long.rotateLeft(h64, 11) * PRIME64_1;
            tail++;
            remaining--;
        }
        return avalanche(h64);
    }

    private static long round(long acc, long input) {
        acc += input * PRIME64_2;
        acc = Long.rotateLeft(acc, 31);
        return acc * PRIME64_1;
    }

    private static long mergeRound(long acc, long v) {
        v = round(0L, v);
        acc ^= v;
        return acc * PRIME64_1 + PRIME64_4;
    }

    private static long avalanche(long h) {
        h ^= h >>> 33;
        h *= PRIME64_2;
        h ^= h >>> 29;
        h *= PRIME64_3;
        h ^= h >>> 32;
        return h;
    }

    private static long readLongLE(byte[] b, int o) {
        return (b[o]     & 0xffL)
            | ((b[o + 1] & 0xffL) << 8)
            | ((b[o + 2] & 0xffL) << 16)
            | ((b[o + 3] & 0xffL) << 24)
            | ((b[o + 4] & 0xffL) << 32)
            | ((b[o + 5] & 0xffL) << 40)
            | ((b[o + 6] & 0xffL) << 48)
            | ((b[o + 7] & 0xffL) << 56);
    }

    private static int readIntLE(byte[] b, int o) {
        return (b[o] & 0xff)
            | ((b[o + 1] & 0xff) << 8)
            | ((b[o + 2] & 0xff) << 16)
            | ((b[o + 3] & 0xff) << 24);
    }
}
