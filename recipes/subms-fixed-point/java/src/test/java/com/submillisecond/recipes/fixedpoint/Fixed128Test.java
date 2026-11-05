package com.submillisecond.recipes.fixedpoint;

import org.junit.jupiter.api.Test;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.RoundingMode;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class Fixed128Test {

    private static Fixed f(long whole, long nanos) {
        return Fixed.fromParts(whole, nanos);
    }

    private static Fixed128 wide(BigInteger raw) {
        return new Fixed128(raw.shiftRight(64).longValueExact(), raw.longValue());
    }

    private static FixedException.Kind kind(Runnable r) {
        return assertThrows(FixedException.class, r::run).kind();
    }

    @Test
    void wideningAndProductAreExact() {
        Fixed x = f(100, 250_000_000);
        assertEquals(BigInteger.valueOf(100_250_000_000L).multiply(BigInteger.valueOf(Fixed.SCALE)),
                Fixed128.fromFixed(x).toBigInteger());
        Fixed128 p = Fixed128.product(f(100, 250_000_000), Fixed.fromInt(300));
        assertEquals(BigInteger.valueOf(30_075).multiply(Fixed128.WIDE_SCALE), p.toBigInteger());
        Fixed tiny = Fixed.fromRaw(Fixed.SCALE + 1);
        assertEquals(Fixed128.WIDE_SCALE.add(BigInteger.valueOf(2L * Fixed.SCALE + 1)),
                Fixed128.product(tiny, tiny).toBigInteger());
        assertEquals(18, Fixed128.WIDE_DECIMALS);
        assertEquals(Fixed128.ZERO, new Fixed128(0, 0));
    }

    @Test
    void accumulatingProductsSurvivesWhatOverflowsLong() {
        Fixed big = Fixed.fromInt(4_000_000_000L);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> big.mul(big, Rounding.HALF_EVEN)));
        Fixed128 acc = Fixed128.ZERO;
        for (int i = 0; i < 10; i++) {
            acc = acc.addProduct(big, big);
        }
        BigInteger want = new BigInteger("16000000000000000000").multiply(Fixed128.WIDE_SCALE).multiply(BigInteger.TEN);
        assertEquals(want, acc.toBigInteger());
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed128.product(big, big).toFixed(Rounding.HALF_EVEN)));
    }

    @Test
    void narrowingRoundsTheDroppedNineDigitsOnce() {
        Fixed128 halfNano = new Fixed128(0, 500_000_000L);
        assertEquals(Fixed.ZERO, halfNano.toFixed(Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(1), halfNano.toFixed(Rounding.HALF_UP));
        assertEquals(Fixed.fromRaw(1), halfNano.toFixed(Rounding.CEILING));
        assertEquals(Fixed.fromRaw(-1), halfNano.neg().toFixed(Rounding.FLOOR));
        assertEquals(Fixed.fromRaw(2), new Fixed128(0, 1_500_000_000L).toFixed(Rounding.HALF_EVEN));
        Fixed x = f(123, 456_789_123);
        assertEquals(x, Fixed128.fromFixed(x).toFixed(Rounding.DOWN));
        assertEquals(x.neg(), Fixed128.fromFixed(x).neg().toFixed(Rounding.DOWN));
    }

    @Test
    void checkedArithmeticAtThe128BitEdge() {
        Fixed128 top = new Fixed128(Long.MAX_VALUE, -1L);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> top.add(new Fixed128(0, 1))));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> top.addFixed(Fixed.fromRaw(1))));
        Fixed128 bottom = new Fixed128(Long.MIN_VALUE, 0);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> bottom.sub(new Fixed128(0, 1))));
        assertEquals(FixedException.Kind.OVERFLOW, kind(bottom::neg));
        assertEquals(new Fixed128(0, 7), new Fixed128(-1, -7).neg());
        assertEquals(Fixed128.fromFixed(f(1, 0)), Fixed128.ZERO.addFixed(f(1, 0)));
        assertEquals(Fixed128.fromFixed(f(2, 0)), Fixed128.fromFixed(f(3, 0)).sub(Fixed128.fromFixed(f(1, 0))));
        assertEquals(FixedException.Kind.DIVISION_BY_ZERO, kind(() -> Fixed128.ZERO.divInt(0, Rounding.HALF_EVEN)));
        // A carry out of the low word.
        assertEquals(new Fixed128(1, 0), new Fixed128(0, -1L).add(new Fixed128(0, 1)));
        assertEquals(new Fixed128(0, -1L), new Fixed128(1, 0).sub(new Fixed128(0, 1)));
    }

    @Test
    void divisionByCountRoundsAtEighteenPlaces() {
        Fixed128 one = Fixed128.fromFixed(Fixed.ONE);
        Fixed128 third = one.divInt(3, Rounding.HALF_EVEN);
        assertEquals(new BigInteger("333333333333333333"), third.toBigInteger());
        assertEquals(new BigInteger("333333333333333334"), one.divInt(3, Rounding.CEILING).toBigInteger());
        assertEquals(Fixed.fromRaw(333_333_333), third.toFixed(Rounding.HALF_EVEN));
        assertEquals(new BigInteger("-333333333333333333"), one.divInt(-3, Rounding.HALF_EVEN).toBigInteger());
        assertEquals(new BigInteger("-333333333333333334"), one.divInt(-3, Rounding.FLOOR).toBigInteger());
        // A quotient wider than 64 bits.
        Fixed128 huge = new Fixed128(1L << 40, 0);
        assertEquals(new Fixed128(1L << 39, 0), huge.divInt(2, Rounding.HALF_EVEN));
        assertEquals(new Fixed128(-(1L << 39), 0), huge.divInt(-2, Rounding.HALF_EVEN));
    }

    @Test
    void divIntMatchesBigDecimalOnRandomValues() {
        Random rnd = new Random(17);
        for (int i = 0; i < 50_000; i++) {
            BigInteger v = new BigInteger(rnd.nextInt(126), rnd);
            if (rnd.nextBoolean()) {
                v = v.negate();
            }
            long n = rnd.nextLong() >> rnd.nextInt(63);
            if (n == 0) {
                continue;
            }
            Fixed128 x = wide(v);
            assertEquals(v, x.toBigInteger());
            for (Rounding m : Rounding.values()) {
                BigInteger want = new BigDecimal(v).divide(BigDecimal.valueOf(n), 0, RoundingMode.valueOf(m.name()))
                        .toBigIntegerExact();
                final BigInteger vv = v;
                assertEquals(want, x.divInt(n, m).toBigInteger(), () -> vv + " / " + n + " " + m);
            }
        }
    }

    @Test
    void predicatesCompareAndToString() {
        assertTrue(Fixed128.ZERO.isZero());
        assertTrue(new Fixed128(-1, -1L).isNegative());
        assertEquals(-1, new Fixed128(-1, -1L).signum());
        assertEquals(1, new Fixed128(0, 9).signum());
        assertEquals(0, Fixed128.ZERO.signum());
        assertEquals("100.5", Fixed128.fromFixed(f(100, 500_000_000)).toString());
        assertEquals("0.000000000000000001", new Fixed128(0, 1).toString());
        assertEquals("-0.000000000000000001", new Fixed128(0, 1).neg().toString());
        assertEquals("0", Fixed128.ZERO.toString());
        assertEquals("1", Fixed128.fromFixed(Fixed.ONE).toString());
        assertTrue(new Fixed128(0, 1).compareTo(new Fixed128(0, 2)) < 0);
        assertTrue(new Fixed128(0, -1L).compareTo(new Fixed128(1, 0)) < 0);
        assertTrue(new Fixed128(-1, 0).compareTo(new Fixed128(0, 0)) < 0);
        assertFalse(new Fixed128(0, 5).isNegative());
    }

    @Test
    void stressExactAccumulationAgainstABigIntegerOracle() {
        Random rnd = new Random(23);
        Fixed128 acc = Fixed128.ZERO;
        BigInteger oracle = BigInteger.ZERO;
        for (int i = 0; i < 200_000; i++) {
            Fixed px = Fixed.fromRaw(Math.floorMod(rnd.nextLong(), 200_000_000_000L));
            Fixed qty = Fixed.fromInt(Math.floorMod(rnd.nextLong(), 10_000));
            acc = acc.addProduct(px, qty);
            oracle = oracle.add(BigInteger.valueOf(px.raw()).multiply(BigInteger.valueOf(qty.raw())));
        }
        assertEquals(oracle, acc.toBigInteger());
        Fixed128 avg = acc.divInt(200_000, Rounding.FLOOR);
        BigInteger[] qr = oracle.divideAndRemainder(BigInteger.valueOf(200_000));
        assertEquals(qr[0].divide(BigInteger.valueOf(Fixed.SCALE)).longValueExact(),
                avg.toFixed(Rounding.FLOOR).raw());
    }
}
