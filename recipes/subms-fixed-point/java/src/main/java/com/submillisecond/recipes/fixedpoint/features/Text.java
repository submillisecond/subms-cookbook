package com.submillisecond.recipes.fixedpoint.features;

import com.submillisecond.recipes.fixedpoint.Fixed;
import com.submillisecond.recipes.fixedpoint.FixedException;

import java.nio.charset.StandardCharsets;

/** Decimal text without allocation: the wire boundary. */
public final class Text {

    /** Longest text {@link #writeTo} can produce: sign, 19 integer digits, point, 9 fractional digits. */
    public static final int MAX_TEXT_LEN = 1 + 19 + 1 + 9;

    private Text() {}

    /** Parse {@code [-|+]digits[.digits]}. */
    public static Fixed parse(CharSequence s) {
        return parse(s, 0, s.length());
    }

    /** {@link #parse(CharSequence)} over a window of a larger sequence. */
    public static Fixed parse(CharSequence s, int from, int to) {
        int i = from;
        boolean negative = false;
        if (i < to) {
            char c = s.charAt(i);
            if (c == '-') {
                negative = true;
                i++;
            } else if (c == '+') {
                i++;
            }
        }
        if (i >= to) {
            throw new FixedException(FixedException.Kind.INVALID_TEXT);
        }
        Acc a = new Acc();
        for (; i < to; i++) {
            a.feed(s.charAt(i));
        }
        return a.finish(negative);
    }

    /** {@link #parse(CharSequence)} over raw ASCII bytes, for a wire buffer. */
    public static Fixed parseBytes(byte[] b, int from, int to) {
        int i = from;
        boolean negative = false;
        if (i < to) {
            byte c = b[i];
            if (c == '-') {
                negative = true;
                i++;
            } else if (c == '+') {
                i++;
            }
        }
        if (i >= to) {
            throw new FixedException(FixedException.Kind.INVALID_TEXT);
        }
        Acc a = new Acc();
        for (; i < to; i++) {
            a.feed((char) (b[i] & 0xFF));
        }
        return a.finish(negative);
    }

    /** {@link #parseBytes(byte[], int, int)} over the whole array. */
    public static Fixed parseBytes(byte[] b) {
        return parseBytes(b, 0, b.length);
    }

    /** The digit accumulator both parse paths share. */
    private static final class Acc {
        long whole;
        long frac;
        int fracDigits;
        int digits;
        boolean seenPoint;

        void feed(char c) {
            if (c >= '0' && c <= '9') {
                digits++;
                int d = c - '0';
                if (seenPoint) {
                    if (fracDigits == Fixed.DECIMALS) {
                        throw new FixedException(FixedException.Kind.TOO_PRECISE);
                    }
                    frac = frac * 10 + d;
                    fracDigits++;
                } else {
                    if (whole > (Long.MAX_VALUE - d) / 10) {
                        throw new FixedException(FixedException.Kind.OVERFLOW);
                    }
                    whole = whole * 10 + d;
                }
            } else if (c == '.' && !seenPoint) {
                seenPoint = true;
            } else {
                throw new FixedException(FixedException.Kind.INVALID_TEXT);
            }
        }

        Fixed finish(boolean negative) {
            if (digits == 0) {
                throw new FixedException(FixedException.Kind.INVALID_TEXT);
            }
            for (int pad = fracDigits; pad < Fixed.DECIMALS; pad++) {
                frac *= 10;
            }
            if (whole > (Long.MAX_VALUE - frac) / Fixed.SCALE) {
                throw new FixedException(FixedException.Kind.OVERFLOW);
            }
            long raw = whole * Fixed.SCALE + frac;
            return Fixed.fromRaw(negative ? -raw : raw);
        }
    }

    /** Render into {@code buf} at {@code offset} with exactly {@code decimals} fractional digits (at most nine; zero writes no point). */
    public static int writeTo(Fixed value, byte[] buf, int offset, int decimals) {
        if (decimals < 0 || decimals > Fixed.DECIMALS) {
            throw new FixedException(FixedException.Kind.TOO_PRECISE);
        }
        long raw = value.raw();
        // |whole| <= 9.2e9 and |frac| < 1e9, so both negations are safe even for MIN_VALUE.
        long whole = Math.abs(raw / Fixed.SCALE);
        long frac = Math.abs(raw % Fixed.SCALE);
        for (int i = decimals; i < Fixed.DECIMALS; i++) {
            frac /= 10;
        }
        int wholeDigits = digitCount(whole);
        int n = (raw < 0 ? 1 : 0) + wholeDigits + (decimals > 0 ? 1 + decimals : 0);
        if (offset < 0 || offset + n > buf.length) {
            throw new FixedException(FixedException.Kind.BUFFER_TOO_SMALL);
        }
        int p = offset;
        if (raw < 0) {
            buf[p++] = '-';
        }
        for (int i = wholeDigits - 1; i >= 0; i--) {
            buf[p + i] = (byte) ('0' + whole % 10);
            whole /= 10;
        }
        p += wholeDigits;
        if (decimals > 0) {
            buf[p++] = '.';
            for (int i = decimals - 1; i >= 0; i--) {
                buf[p + i] = (byte) ('0' + frac % 10);
                frac /= 10;
            }
        }
        return n;
    }

    /** {@link #writeTo} into a fresh String. */
    public static String toStringDecimals(Fixed value, int decimals) {
        byte[] buf = new byte[MAX_TEXT_LEN];
        int n = writeTo(value, buf, 0, decimals);
        return new String(buf, 0, n, StandardCharsets.US_ASCII);
    }

    private static int digitCount(long v) {
        int n = 1;
        while (v >= 10) {
            v /= 10;
            n++;
        }
        return n;
    }
}
