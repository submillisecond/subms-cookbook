package com.submillisecond.recipes.fixedpoint;

/** An exact decimal at nine fixed places, stored as a signed 64-bit integer. */
public record Fixed(long raw) implements Comparable<Fixed> {

    /** Fractional digits every {@code Fixed} carries. */
    public static final int DECIMALS = 9;
    /** {@code 10^DECIMALS}: the raw value of 1. */
    public static final long SCALE = 1_000_000_000L;

    public static final Fixed ZERO = new Fixed(0);
    public static final Fixed ONE = new Fixed(SCALE);
    public static final Fixed MAX = new Fixed(Long.MAX_VALUE);
    public static final Fixed MIN = new Fixed(Long.MIN_VALUE);

    private static final long[] POW10 = {
        1L,
        10L,
        100L,
        1_000L,
        10_000L,
        100_000L,
        1_000_000L,
        10_000_000L,
        100_000_000L,
        1_000_000_000L,
        10_000_000_000L,
        100_000_000_000L,
        1_000_000_000_000L,
        10_000_000_000_000L,
        100_000_000_000_000L,
        1_000_000_000_000_000L,
        10_000_000_000_000_000L,
        100_000_000_000_000_000L,
        1_000_000_000_000_000_000L,
    };

    /** Wrap a raw value already at nine decimals. */
    public static Fixed fromRaw(long raw) {
        return new Fixed(raw);
    }

    /** A whole number. */
    public static Fixed fromInt(long n) {
        return new Fixed(Int128.mulExact(n, SCALE));
    }

    /** A whole part plus a count of billionths. */
    public static Fixed fromParts(long whole, long nanos) {
        if (nanos < 0 || nanos >= SCALE) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        long w = Int128.mulExact(whole, SCALE);
        return new Fixed(Int128.addExact(w, whole < 0 ? -nanos : nanos));
    }

    /** Rescale an integer quoted at {@code decimals} places into nine. */
    public static Fixed fromScaled(long value, int decimals, Rounding mode) {
        if (decimals < 0 || decimals > 18) {
            throw new FixedException(FixedException.Kind.TOO_PRECISE);
        }
        if (decimals <= DECIMALS) {
            return new Fixed(Int128.mulExact(value, POW10[DECIMALS - decimals]));
        }
        return new Fixed(mode.div(value, POW10[decimals - DECIMALS]));
    }

    /** The value as an integer at {@code decimals} places. */
    public long toScaled(int decimals, Rounding mode) {
        if (decimals < 0 || decimals > 18) {
            throw new FixedException(FixedException.Kind.TOO_PRECISE);
        }
        if (decimals >= DECIMALS) {
            return Int128.mulExact(raw, POW10[decimals - DECIMALS]);
        }
        return mode.div(raw, POW10[DECIMALS - decimals]);
    }

    /** Convert a double at the boundary. */
    public static Fixed fromDouble(double x, Rounding mode) {
        if (!Double.isFinite(x)) {
            throw new FixedException(FixedException.Kind.NOT_FINITE);
        }
        long bits = Double.doubleToRawLongBits(x);
        boolean negative = bits < 0;
        int expBits = (int) ((bits >>> 52) & 0x7FF);
        long fracBits = bits & ((1L << 52) - 1);
        long m;
        int e;
        if (expBits == 0) {
            m = fracBits;
            e = -1074;
        } else {
            m = fracBits | (1L << 52);
            e = expBits - 1075;
        }
        if (m == 0) {
            return ZERO;
        }
        // m * 10^9 is at most 83 bits: hold it as (hi, lo).
        long hi = Math.multiplyHigh(m, SCALE);
        long lo = m * SCALE;
        if (e >= 0) {
            if (e > 40) {
                throw new FixedException(FixedException.Kind.OVERFLOW);
            }
            long shi = e == 0 ? hi : (hi << e) | (lo >>> (64 - e));
            long slo = lo << e;
            if (shi != 0 || slo < 0) {
                throw new FixedException(FixedException.Kind.OVERFLOW);
            }
            return new Fixed(negative ? -slo : slo);
        }
        // Past 2^120 only the remainder's sign matters, so the clamp changes no mode's answer.
        int k = Math.min(-e, 120);
        long q;
        boolean halfBit;
        boolean rest;
        if (k >= 64) {
            int kk = k - 64;
            q = kk == 0 ? hi : hi >>> kk;
            long dropped = kk == 0 ? 0 : hi & ((1L << kk) - 1);
            halfBit = kk == 0 ? lo < 0 : ((hi >>> (kk - 1)) & 1) != 0;
            rest = kk == 0 ? (lo << 1) != 0 : dropped != (halfBit ? 1L << (kk - 1) : 0) || lo != 0;
        } else {
            if ((hi >>> k) != 0) {
                throw new FixedException(FixedException.Kind.OVERFLOW);
            }
            q = (hi << (64 - k)) | (lo >>> k);
            long dropped = lo & ((1L << k) - 1);
            halfBit = ((lo >>> (k - 1)) & 1) != 0;
            rest = dropped != (halfBit ? 1L << (k - 1) : 0);
        }
        if (halfBit || rest) {
            int half = halfBit ? (rest ? 1 : 0) : -1;
            if (mode.away(negative, half, (q & 1) != 0)) {
                q += 1;
            }
        }
        if (q < 0) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return new Fixed(negative ? -q : q);
    }

    /** The value as a double. */
    public double toDouble() {
        return raw / (double) SCALE;
    }

    /** The whole part, truncated toward zero. */
    public long intPart() {
        return raw / SCALE;
    }

    /** The fractional part in billionths, carrying the value's sign. */
    public long fracPart() {
        return raw % SCALE;
    }

    public boolean isZero() {
        return raw == 0;
    }

    public boolean isNegative() {
        return raw < 0;
    }

    public boolean isPositive() {
        return raw > 0;
    }

    /** {@code -1}, {@code 0} or {@code 1}. */
    public int signum() {
        return Long.signum(raw);
    }

    /** OVERFLOW only for {@code MIN}, whose magnitude has no positive twin. */
    public Fixed abs() {
        return raw < 0 ? neg() : this;
    }

    public Fixed neg() {
        if (raw == Long.MIN_VALUE) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return new Fixed(-raw);
    }

    public Fixed add(Fixed rhs) {
        return new Fixed(Int128.addExact(raw, rhs.raw));
    }

    public Fixed sub(Fixed rhs) {
        return new Fixed(Int128.subExact(raw, rhs.raw));
    }

    public Fixed saturatingAdd(Fixed rhs) {
        long r = raw + rhs.raw;
        if (((raw ^ r) & (rhs.raw ^ r)) < 0) {
            return raw < 0 ? MIN : MAX;
        }
        return new Fixed(r);
    }

    public Fixed saturatingSub(Fixed rhs) {
        long r = raw - rhs.raw;
        if (((raw ^ rhs.raw) & (raw ^ r)) < 0) {
            return raw < 0 ? MIN : MAX;
        }
        return new Fixed(r);
    }

    /** {@code this * rhs}, the 128-bit product rounded back to nine decimals once. */
    public Fixed mul(Fixed rhs, Rounding mode) {
        long hi = Math.multiplyHigh(raw, rhs.raw);
        long lo = raw * rhs.raw;
        return new Fixed(Int128.divRound(hi, lo, SCALE, mode));
    }

    /** {@code this * n} for a plain integer count. */
    public Fixed mulInt(long n) {
        return new Fixed(Int128.mulExact(raw, n));
    }

    /** {@code this / rhs}, rounded once. */
    public Fixed div(Fixed rhs, Rounding mode) {
        long hi = Math.multiplyHigh(raw, SCALE);
        long lo = raw * SCALE;
        return new Fixed(Int128.divRound(hi, lo, rhs.raw, mode));
    }

    /** {@code this / n} for a plain integer count, rounded once. */
    public Fixed divInt(long n, Rounding mode) {
        if (n == 0) {
            throw new FixedException(FixedException.Kind.DIVISION_BY_ZERO);
        }
        if (n == -1 && raw == Long.MIN_VALUE) {
            throw new FixedException(FixedException.Kind.OVERFLOW);
        }
        return new Fixed(mode.div(raw, n));
    }

    /** Round to {@code decimals} places (at most nine) under {@code mode}. */
    public Fixed round(int decimals, Rounding mode) {
        if (decimals >= DECIMALS) {
            return this;
        }
        long d = POW10[DECIMALS - Math.max(decimals, 0)];
        // The rounded quotient times d is at most |raw| + d, which fits.
        return new Fixed(mode.div(raw, d) * d);
    }

    /** Snap onto a price grid: the nearest multiple of {@code tick} under {@code mode}. */
    public Fixed roundToTick(Fixed tick, Rounding mode) {
        if (tick.raw <= 0) {
            throw new FixedException(FixedException.Kind.INVALID_TICK);
        }
        long q = mode.div(raw, tick.raw);
        return new Fixed(Int128.mulExact(q, tick.raw));
    }

    /** Whether the value is an exact multiple of {@code tick}. */
    public boolean isOnTick(Fixed tick) {
        return tick.raw > 0 && raw % tick.raw == 0;
    }

    /** How many ticks the value is. */
    public long ticks(Fixed tick) {
        if (tick.raw <= 0) {
            throw new FixedException(FixedException.Kind.INVALID_TICK);
        }
        if (raw % tick.raw != 0) {
            throw new FixedException(FixedException.Kind.OFF_TICK);
        }
        return raw / tick.raw;
    }

    public static Fixed min(Fixed a, Fixed b) {
        return a.raw <= b.raw ? a : b;
    }

    public static Fixed max(Fixed a, Fixed b) {
        return a.raw >= b.raw ? a : b;
    }

    @Override
    public int compareTo(Fixed o) {
        return Long.compare(raw, o.raw);
    }

    /** Shortest exact decimal: {@code 100.5}, not {@code 100.500000000}; {@code -0.000000001} keeps its sign. */
    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder(24);
        appendTo(sb);
        return sb.toString();
    }

    /** {@link #toString()} into an existing builder. */
    public void appendTo(StringBuilder sb) {
        long whole = raw / SCALE;
        long frac = raw % SCALE;
        if (raw < 0) {
            sb.append('-');
        }
        // Long.MIN_VALUE's magnitude: append the unsigned forms.
        sb.append(Long.toUnsignedString(whole < 0 ? -whole : whole));
        long f = frac < 0 ? -frac : frac;
        if (f == 0) {
            return;
        }
        int digits = DECIMALS;
        while (f % 10 == 0) {
            f /= 10;
            digits--;
        }
        sb.append('.');
        String s = Long.toString(f);
        for (int i = s.length(); i < digits; i++) {
            sb.append('0');
        }
        sb.append(s);
    }
}
