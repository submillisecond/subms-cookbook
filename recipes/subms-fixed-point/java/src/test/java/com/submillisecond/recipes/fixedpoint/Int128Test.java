package com.submillisecond.recipes.fixedpoint;

import org.junit.jupiter.api.Test;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.RoundingMode;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class Int128Test {

    private static BigInteger big(long hi, long lo) {
        return BigInteger.valueOf(hi).shiftLeft(64).add(new BigInteger(Long.toUnsignedString(lo)));
    }

    @Test
    void unsignedLongDivisionMatchesBigIntegerAcrossShapes() {
        Random rnd = new Random(7);
        for (int i = 0; i < 300_000; i++) {
            long v = rnd.nextLong() >>> rnd.nextInt(63);
            if (v == 0) {
                continue;
            }
            long u1 = Long.remainderUnsigned(rnd.nextLong(), v); // u1 < v by construction
            long u0 = rnd.nextLong();
            BigInteger want = big(0, u1).shiftLeft(64).add(new BigInteger(Long.toUnsignedString(u0)))
                    .divide(new BigInteger(Long.toUnsignedString(v)));
            long got = Int128.udiv128by64(u1, u0, v);
            assertEquals(want, new BigInteger(Long.toUnsignedString(got)), () -> u1 + ":" + u0 + " / " + v);
        }
    }

    @Test
    void signedDivRoundMatchesBigDecimalUnderEveryMode() {
        Random rnd = new Random(11);
        for (int i = 0; i < 100_000; i++) {
            long a = rnd.nextLong() >> rnd.nextInt(60);
            long b = rnd.nextLong() >> rnd.nextInt(60);
            long d = rnd.nextLong() >> rnd.nextInt(60);
            if (d == 0) {
                continue;
            }
            long hi = Math.multiplyHigh(a, b);
            long lo = a * b;
            BigInteger n = BigInteger.valueOf(a).multiply(BigInteger.valueOf(b));
            assertEquals(n, big(hi, lo));
            for (Rounding m : Rounding.values()) {
                BigInteger want = new BigDecimal(n)
                        .divide(BigDecimal.valueOf(d), 0, RoundingMode.valueOf(m.name()))
                        .toBigIntegerExact();
                if (want.bitLength() > 63) {
                    assertThrows(FixedException.class, () -> Int128.divRound(hi, lo, d, m));
                } else {
                    assertEquals(want.longValueExact(), Int128.divRound(hi, lo, d, m),
                            () -> a + "*" + b + " / " + d + " " + m);
                }
            }
        }
    }

    @Test
    void divRoundEdges() {
        assertThrows(FixedException.class, () -> Int128.divRound(0, 1, 0, Rounding.HALF_EVEN));
        assertEquals(FixedException.Kind.DIVISION_BY_ZERO,
                assertThrows(FixedException.class, () -> Int128.divRound(0, 1, 0, Rounding.DOWN)).kind());
        // Long.MIN_VALUE / -1 overflows; MIN / 1 does not.
        assertThrows(FixedException.class, () -> Int128.divRound(-1, Long.MIN_VALUE, -1, Rounding.DOWN));
        assertEquals(Long.MIN_VALUE, Int128.divRound(-1, Long.MIN_VALUE, 1, Rounding.DOWN));
        // 2^63 exactly: too big positive, fine negative after rounding away.
        assertThrows(FixedException.class, () -> Int128.divRound(0, Long.MIN_VALUE, 1, Rounding.DOWN));
        assertEquals(Long.MIN_VALUE, Int128.divRound(0, Long.MIN_VALUE, -1, Rounding.DOWN));
        // Divisor Long.MIN_VALUE: magnitude 2^63 as unsigned.
        assertEquals(-1, Int128.divRound(0, Long.MIN_VALUE, Long.MIN_VALUE, Rounding.DOWN));
        assertEquals(-1, Int128.divRound(0, Long.MAX_VALUE, Long.MIN_VALUE, Rounding.HALF_DOWN));
        // 2^62 / -2^63 is an exact tie: half-down and half-even stay at 0, half-up goes to -1.
        assertEquals(0, Int128.divRound(0, 1L << 62, Long.MIN_VALUE, Rounding.HALF_DOWN));
        assertEquals(0, Int128.divRound(0, 1L << 62, Long.MIN_VALUE, Rounding.HALF_EVEN));
        assertEquals(-1, Int128.divRound(0, 1L << 62, Long.MIN_VALUE, Rounding.HALF_UP));
        // Quotient at the top of the range after a round-away: overflow.
        assertEquals(Long.MAX_VALUE, Int128.divRound(0, Long.MAX_VALUE, 1, Rounding.UP));
        assertThrows(FixedException.class, () -> Int128.divRound(1, 0, 2, Rounding.UP));
    }

    @Test
    void exactHelpersThrowTypedOverflow() {
        assertEquals(6, Int128.mulExact(2, 3));
        assertEquals(Long.MIN_VALUE, Int128.mulExact(Long.MIN_VALUE, 1));
        assertThrows(FixedException.class, () -> Int128.mulExact(Long.MIN_VALUE, -1));
        assertThrows(FixedException.class, () -> Int128.mulExact(Long.MAX_VALUE, 2));
        assertEquals(Long.MAX_VALUE, Int128.addExact(Long.MAX_VALUE - 1, 1));
        assertThrows(FixedException.class, () -> Int128.addExact(Long.MAX_VALUE, 1));
        assertThrows(FixedException.class, () -> Int128.addExact(Long.MIN_VALUE, -1));
        assertEquals(Long.MIN_VALUE, Int128.subExact(Long.MIN_VALUE + 1, 1));
        assertThrows(FixedException.class, () -> Int128.subExact(Long.MIN_VALUE, 1));
        assertThrows(FixedException.class, () -> Int128.subExact(Long.MAX_VALUE, -1));
    }
}
