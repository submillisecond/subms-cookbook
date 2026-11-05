package com.submillisecond.recipes.fixedpoint;

import java.math.BigInteger;

/** An exact decimal at eighteen places in 128 bits: the accumulator type. */
public record Fixed128(long hi, long lo) implements Comparable<Fixed128> {

    /** Fractional digits a {@code Fixed128} carries: the product scale. */
    public static final int WIDE_DECIMALS = 18;
    /** {@code 10^18}. */
    public static final BigInteger WIDE_SCALE = BigInteger.TEN.pow(WIDE_DECIMALS);

    public static final Fixed128 ZERO = new Fixed128(0, 0);

    /** Widen a {@link Fixed}. */
    public static Fixed128 fromFixed(Fixed x) {
        return new Fixed128(Math.multiplyHigh(x.raw(), Fixed.SCALE), x.raw() * Fixed.SCALE);
    }

    /** {@code a * b} with no rounding at all. */
    public static Fixed128 product(Fixed a, Fixed b) {
        return new Fixed128(Math.multiplyHigh(a.raw(), b.raw()), a.raw() * b.raw());
    }

    public Fixed128 add(Fixed128 rhs) {
        long l = lo + rhs.lo;
        long carry = Long.compareUnsigned(l, lo) < 0 ? 1 : 0;
        long h = hi + rhs.hi + carry;
        // Same-sign operands whose sum flips sign: the one way 128-bit add overflows.
        if (((hi ^ h) & (rhs.hi ^ h)) < 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return new Fixed128(h, l);
    }

    public Fixed128 sub(Fixed128 rhs) {
        return add(rhs.neg());
    }

    /** Add a nine-decimal value without converting it first. */
    public Fixed128 addFixed(Fixed rhs) {
        return add(fromFixed(rhs));
    }

    /** Accumulate {@code a * b} exactly: the fill loop's one call. */
    public Fixed128 addProduct(Fixed a, Fixed b) {
        return add(product(a, b));
    }

    public Fixed128 neg() {
        if (hi == Long.MIN_VALUE && lo == 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        long l = -lo;
        long h = ~hi + (lo == 0 ? 1 : 0);
        return new Fixed128(h, l);
    }

    /** Divide by an integer count, rounded at eighteen places: the exact average of {@code n} notionals. */
    public Fixed128 divInt(long n, Rounding mode) {
        if (n == 0) {
            throw new FixedException(FixedException.Kind.DIVISION_BY_ZERO);
        }
        boolean negative = isNegative() != (n < 0);
        Fixed128 mag = isNegative() ? neg() : this;
        long d = n < 0 ? -n : n; // MIN_VALUE stays 2^63 unsigned, which is right.
        long qhi = Long.divideUnsigned(mag.hi, d);
        long r1 = Long.remainderUnsigned(mag.hi, d);
        long qlo = Int128.udiv128by64(r1, mag.lo, d);
        long r = mag.lo - qlo * d;
        if (r != 0) {
            int half = Long.compareUnsigned(r << 1, d);
            if (mode.away(negative, half, (qlo & 1) != 0)) {
                qlo += 1;
                if (qlo == 0) {
                    qhi += 1;
                }
            }
        }
        Fixed128 q = new Fixed128(qhi, qlo);
        return negative ? q.neg() : q;
    }

    /** Narrow to nine decimals, rounding the nine dropped digits once. */
    public Fixed toFixed(Rounding mode) {
        return new Fixed(Int128.divRound(hi, lo, Fixed.SCALE, mode));
    }

    public boolean isZero() {
        return hi == 0 && lo == 0;
    }

    public boolean isNegative() {
        return hi < 0;
    }

    public int signum() {
        return hi < 0 ? -1 : (hi == 0 && lo == 0) ? 0 : 1;
    }

    /** The exact value as a {@code BigInteger} at eighteen places. */
    public BigInteger toBigInteger() {
        return BigInteger.valueOf(hi).shiftLeft(64).add(new BigInteger(Long.toUnsignedString(lo)));
    }

    @Override
    public int compareTo(Fixed128 o) {
        int c = Long.compare(hi, o.hi);
        return c != 0 ? c : Long.compareUnsigned(lo, o.lo);
    }

    /** Shortest exact decimal, like {@link Fixed}. */
    @Override
    public String toString() {
        BigInteger v = toBigInteger();
        boolean negative = v.signum() < 0;
        BigInteger[] qr = v.abs().divideAndRemainder(WIDE_SCALE);
        StringBuilder sb = new StringBuilder(48);
        if (negative) {
            sb.append('-');
        }
        sb.append(qr[0]);
        if (qr[1].signum() != 0) {
            String frac = qr[1].toString();
            StringBuilder f = new StringBuilder(WIDE_DECIMALS);
            for (int i = frac.length(); i < WIDE_DECIMALS; i++) {
                f.append('0');
            }
            f.append(frac);
            int end = f.length();
            while (f.charAt(end - 1) == '0') {
                end--;
            }
            sb.append('.').append(f, 0, end);
        }
        return sb.toString();
    }
}
