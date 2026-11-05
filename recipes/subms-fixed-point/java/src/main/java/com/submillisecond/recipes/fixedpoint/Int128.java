package com.submillisecond.recipes.fixedpoint;

/** The 128-bit integer arithmetic a multiply or a divide widens through. */
final class Int128 {

    private Int128() {}

    /** Signed {@code (hi:lo) / d}, rounded under {@code mode}. */
    static long divRound(long hi, long lo, long d, Rounding mode) {
        if (d == 0) {
            throw new FixedException(FixedException.Kind.DIVISION_BY_ZERO);
        }
        boolean nNeg = hi < 0;
        boolean dNeg = d < 0;
        long mhi = hi;
        long mlo = lo;
        if (nNeg) {
            mlo = -lo;
            mhi = ~hi + (lo == 0 ? 1 : 0);
        }
        long dabs = dNeg ? -d : d; // MIN_VALUE stays MIN_VALUE: 2^63 as unsigned, correct.
        if (Long.compareUnsigned(mhi, dabs) >= 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        long q = udiv128by64(mhi, mlo, dabs);
        long r = mlo - q * dabs; // exact: u = q*v + r with r < v < 2^64
        boolean negative = nNeg != dNeg;
        if (r != 0) {
            int half = Long.compareUnsigned(r << 1, dabs);
            // r < dabs <= 2^63, so 2r fits an unsigned long.
            if (mode.away(negative, half, (q & 1) != 0)) {
                q += 1;
            }
        }
        // The magnitude must fit a signed long: up to 2^63 - 1 positive, 2^63 negative.
        if (negative) {
            if (Long.compareUnsigned(q, Long.MIN_VALUE) > 0) {
                throw new FixedException(FixedException.Kind.OVERFLOW);
            }
            return -q;
        }
        if (q < 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return q;
    }

    /** Unsigned {@code (u1:u0) / v} where the caller guarantees {@code u1 < v} so the quotient fits 64 bits. */
    static long udiv128by64(long u1, long u0, long v) {
        final long b = 1L << 32;
        int s = Long.numberOfLeadingZeros(v);
        long vn = v << s;
        long vn1 = vn >>> 32;
        long vn0 = vn & 0xFFFF_FFFFL;
        long un32 = s == 0 ? u1 : (u1 << s) | (u0 >>> (64 - s));
        long un10 = u0 << s;
        long un1 = un10 >>> 32;
        long un0 = un10 & 0xFFFF_FFFFL;

        long q1 = Long.divideUnsigned(un32, vn1);
        long rhat = un32 - q1 * vn1;
        while (Long.compareUnsigned(q1, b) >= 0
                || Long.compareUnsigned(q1 * vn0, (rhat << 32) | un1) > 0) {
            q1 -= 1;
            rhat += vn1;
            if (Long.compareUnsigned(rhat, b) >= 0) {
                break;
            }
        }
        long un21 = (un32 << 32) + un1 - q1 * vn;
        long q0 = Long.divideUnsigned(un21, vn1);
        rhat = un21 - q0 * vn1;
        while (Long.compareUnsigned(q0, b) >= 0
                || Long.compareUnsigned(q0 * vn0, (rhat << 32) | un0) > 0) {
            q0 -= 1;
            rhat += vn1;
            if (Long.compareUnsigned(rhat, b) >= 0) {
                break;
            }
        }
        return (q1 << 32) | q0;
    }

    /** {@code a * b} exactly, or OVERFLOW. */
    static long mulExact(long a, long b) {
        long hi = Math.multiplyHigh(a, b);
        long lo = a * b;
        if (hi != (lo >> 63)) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return lo;
    }

    /** {@code a + b} exactly, or OVERFLOW. */
    static long addExact(long a, long b) {
        long r = a + b;
        if (((a ^ r) & (b ^ r)) < 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return r;
    }

    /** {@code a - b} exactly, or OVERFLOW. */
    static long subExact(long a, long b) {
        long r = a - b;
        if (((a ^ b) & (a ^ r)) < 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return r;
    }
}
