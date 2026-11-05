package com.submillisecond.recipes.fixedpoint;

import org.junit.jupiter.api.Test;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.RoundingMode;
import java.util.HashSet;
import java.util.Random;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class FixedTest {

    private static Fixed f(long whole, long nanos) {
        return Fixed.fromParts(whole, nanos);
    }

    private static FixedException.Kind kind(Runnable r) {
        return assertThrows(FixedException.class, r::run).kind();
    }

    @Test
    void constructorsAgreeOnTheRawRepresentation() {
        assertEquals(Fixed.ONE, Fixed.fromInt(1));
        assertEquals(Fixed.SCALE, Fixed.ONE.raw());
        assertEquals(100_250_000_000L, f(100, 250_000_000).raw());
        assertEquals(-1_500_000_000L, f(-1, 500_000_000).raw());
        assertEquals(7, Fixed.fromRaw(7).raw());
        assertEquals(9, Fixed.DECIMALS);
    }

    @Test
    void constructorsRefuseWhatDoesNotFit() {
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromInt(Long.MAX_VALUE)));
        assertEquals(9_223_372_036L * Fixed.SCALE, Fixed.fromInt(9_223_372_036L).raw());
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromInt(9_223_372_037L)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromParts(0, Fixed.SCALE)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromParts(0, -1)));
        assertEquals(Fixed.MAX, Fixed.fromParts(9_223_372_036L, 854_775_807));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromParts(9_223_372_036L, 854_775_808)));
    }

    @Test
    void orderingHashingAndEqualityAreTheIntegers() {
        Fixed a = f(100, 0);
        Fixed b = f(100, 1);
        assertTrue(a.compareTo(b) < 0);
        assertEquals(b, Fixed.max(a, b));
        assertEquals(a, Fixed.min(a, b));
        assertEquals(f(1, 500_000_000), Fixed.fromRaw(1_500_000_000L));
        Set<Fixed> set = new HashSet<>();
        set.add(a);
        set.add(b);
        set.add(a);
        assertEquals(2, set.size());
    }

    @Test
    void additionAndSubtractionAreExactAndAssociative() {
        Fixed tenth = f(0, 100_000_000);
        Fixed sum = Fixed.ZERO;
        for (int i = 0; i < 10; i++) {
            sum = sum.add(tenth);
        }
        assertEquals(Fixed.ONE, sum);
        Fixed forward = Fixed.ZERO;
        Fixed backward = Fixed.ZERO;
        for (long k = 1; k <= 1_000; k++) {
            forward = forward.add(Fixed.fromRaw(k));
            backward = backward.add(Fixed.fromRaw(1_001 - k));
        }
        assertEquals(forward, backward);
        assertEquals(500_500, forward.raw());
        assertEquals(f(0, 900_000_000), Fixed.ONE.sub(tenth));
        assertEquals(Fixed.fromRaw(-900_000_000), Fixed.ONE.sub(tenth).neg());
    }

    @Test
    void checkedAndSaturatingArithmeticAtTheEdges() {
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.add(Fixed.fromRaw(1))));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MIN.sub(Fixed.fromRaw(1))));
        assertEquals(FixedException.Kind.OVERFLOW, kind(Fixed.MIN::neg));
        assertEquals(FixedException.Kind.OVERFLOW, kind(Fixed.MIN::abs));
        assertEquals(Fixed.fromRaw(5), Fixed.fromRaw(-5).abs());
        assertEquals(Fixed.fromRaw(5), Fixed.fromRaw(5).abs());
        assertEquals(Fixed.MAX, Fixed.MAX.saturatingAdd(Fixed.ONE));
        assertEquals(Fixed.MIN, Fixed.MIN.saturatingSub(Fixed.ONE));
        assertEquals(Fixed.MIN, Fixed.MIN.saturatingAdd(Fixed.fromRaw(-1)));
        assertEquals(Fixed.MAX, Fixed.MAX.saturatingSub(Fixed.fromRaw(-1)));
        assertEquals(f(3, 0), f(1, 0).saturatingAdd(f(2, 0)));
        assertEquals(f(1, 0), f(3, 0).saturatingSub(f(2, 0)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.mulInt(2)));
        assertEquals(f(6, 0), f(2, 0).mulInt(3));
    }

    @Test
    void multiplicationWidensAndRoundsOnce() {
        Fixed px = f(100, 250_000_000);
        Fixed qty = Fixed.fromInt(300);
        assertEquals(Fixed.fromInt(30_075), px.mul(qty, Rounding.HALF_EVEN));
        Fixed tiny = Fixed.fromRaw(Fixed.SCALE + 1);
        assertEquals(Fixed.fromRaw(Fixed.SCALE + 2), tiny.mul(tiny, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(Fixed.SCALE + 3), tiny.mul(tiny, Rounding.UP));
        Fixed half = f(0, 500_000_000);
        Fixed oneNano = Fixed.fromRaw(1);
        assertEquals(Fixed.ZERO, half.mul(oneNano, Rounding.HALF_EVEN));
        assertEquals(oneNano, half.mul(oneNano, Rounding.HALF_UP));
        assertEquals(oneNano.neg(), half.mul(oneNano.neg(), Rounding.HALF_UP));
        Fixed big = Fixed.fromInt(4_000_000_000L);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> big.mul(big, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.mul(Fixed.MAX, Rounding.HALF_EVEN)));
    }

    @Test
    void divisionRoundsOnceAndRefusesZero() {
        Fixed three = Fixed.fromInt(3);
        assertEquals(Fixed.fromRaw(333_333_333), Fixed.ONE.div(three, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(333_333_334), Fixed.ONE.div(three, Rounding.CEILING));
        assertEquals(Fixed.fromRaw(666_666_667), Fixed.fromInt(2).div(three, Rounding.HALF_EVEN));
        assertEquals(FixedException.Kind.DIVISION_BY_ZERO, kind(() -> Fixed.ONE.div(Fixed.ZERO, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.div(Fixed.fromRaw(1), Rounding.HALF_EVEN)));
        assertEquals(f(2, 500_000_000), f(10, 0).divInt(4, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(2), Fixed.fromRaw(5).divInt(2, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(3), Fixed.fromRaw(5).divInt(2, Rounding.HALF_UP));
        assertEquals(FixedException.Kind.DIVISION_BY_ZERO, kind(() -> Fixed.ONE.divInt(0, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MIN.divInt(-1, Rounding.HALF_EVEN)));
    }

    @Test
    void roundingToFewerDecimals() {
        Fixed x = f(100, 123_456_789);
        assertEquals(f(100, 120_000_000), x.round(2, Rounding.HALF_EVEN));
        assertEquals(f(100, 130_000_000), x.round(2, Rounding.UP));
        assertEquals(f(100, 0), x.round(0, Rounding.FLOOR));
        assertEquals(x, x.round(9, Rounding.HALF_EVEN));
        assertEquals(x, x.round(12, Rounding.HALF_EVEN));
        Fixed y = f(-100, 125_000_000);
        assertEquals(f(-100, 120_000_000), y.round(2, Rounding.HALF_EVEN));
        assertEquals(f(-100, 130_000_000), y.round(2, Rounding.HALF_UP));
        assertEquals(f(-100, 130_000_000), y.round(2, Rounding.FLOOR));
        assertEquals(f(-100, 120_000_000), y.round(2, Rounding.CEILING));
    }

    @Test
    void tickGridRoundCheckAndCount() {
        Fixed cent = Fixed.fromRaw(10_000_000L);
        Fixed quarter = f(0, 250_000_000);
        Fixed px = f(100, 254_000_000);
        assertEquals(f(100, 250_000_000), px.roundToTick(cent, Rounding.HALF_EVEN));
        assertEquals(f(100, 260_000_000), px.roundToTick(cent, Rounding.CEILING));
        assertEquals(f(100, 250_000_000), px.roundToTick(quarter, Rounding.HALF_EVEN));
        assertEquals(f(100, 500_000_000), f(100, 375_000_000).roundToTick(quarter, Rounding.HALF_EVEN));
        assertEquals(f(100, 0), f(100, 125_000_000).roundToTick(quarter, Rounding.HALF_EVEN));
        assertTrue(f(100, 250_000_000).isOnTick(cent));
        assertFalse(px.isOnTick(cent));
        assertFalse(px.isOnTick(Fixed.ZERO));
        assertEquals(FixedException.Kind.INVALID_TICK, kind(() -> px.roundToTick(Fixed.ZERO, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.INVALID_TICK, kind(() -> px.roundToTick(Fixed.fromRaw(-1), Rounding.HALF_EVEN)));
        assertEquals(10_025, f(100, 250_000_000).ticks(cent));
        assertEquals(FixedException.Kind.OFF_TICK, kind(() -> px.ticks(cent)));
        assertEquals(FixedException.Kind.INVALID_TICK, kind(() -> px.ticks(Fixed.ZERO)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.roundToTick(Fixed.fromRaw(10), Rounding.UP)));
    }

    @Test
    void scaleConversionInAndOut() {
        assertEquals(f(100, 250_000_000), Fixed.fromScaled(1_002_500, 4, Rounding.HALF_EVEN));
        assertEquals(1_002_500, f(100, 250_000_000).toScaled(4, Rounding.HALF_EVEN));
        assertEquals(10_026, f(100, 255_000_000).toScaled(2, Rounding.HALF_EVEN));
        assertEquals(10_025, f(100, 255_000_000).toScaled(2, Rounding.DOWN));
        assertEquals(Fixed.fromRaw(Fixed.SCALE), Fixed.fromScaled(1_000_000_000_500L, 12, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(Fixed.SCALE + 1), Fixed.fromScaled(1_000_000_000_500L, 12, Rounding.HALF_UP));
        assertEquals(1_000_000_000_000L, Fixed.ONE.toScaled(12, Rounding.HALF_EVEN));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Fixed.fromScaled(1, 19, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Fixed.ONE.toScaled(19, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Fixed.ONE.toScaled(-1, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromScaled(Long.MAX_VALUE, 0, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.MAX.toScaled(18, Rounding.HALF_EVEN)));
    }

    @Test
    void doubleBoundaryIsExactOnRepresentableValuesAndRoundsTheRest() {
        assertEquals(f(100, 250_000_000), Fixed.fromDouble(100.25, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(-500_000_000), Fixed.fromDouble(-0.5, Rounding.HALF_EVEN));
        assertEquals(Fixed.ZERO, Fixed.fromDouble(0.0, Rounding.HALF_EVEN));
        assertEquals(Fixed.ZERO, Fixed.fromDouble(-0.0, Rounding.HALF_EVEN));
        assertEquals(f(0, 100_000_000), Fixed.fromDouble(0.1, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(100_000_001), Fixed.fromDouble(0.1, Rounding.UP));
        assertEquals(Fixed.ZERO, Fixed.fromDouble(1e-12, Rounding.HALF_EVEN));
        assertEquals(Fixed.fromRaw(1), Fixed.fromDouble(1e-12, Rounding.UP));
        assertEquals(Fixed.fromRaw(1), Fixed.fromDouble(1e-300, Rounding.CEILING));
        assertEquals(Fixed.fromRaw(-1), Fixed.fromDouble(-1e-300, Rounding.FLOOR));
        assertEquals(Fixed.ZERO, Fixed.fromDouble(Double.MIN_VALUE, Rounding.HALF_EVEN));
        assertEquals(FixedException.Kind.NOT_FINITE, kind(() -> Fixed.fromDouble(Double.NaN, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.NOT_FINITE, kind(() -> Fixed.fromDouble(Double.POSITIVE_INFINITY, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromDouble(1e10, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromDouble(1e300, Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromDouble(2.0e19, Rounding.HALF_EVEN)));
        assertEquals(Fixed.fromInt(9_000_000_000L), Fixed.fromDouble(9.0e9, Rounding.HALF_EVEN));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Fixed.fromDouble(Math.scalb(1.0, 40), Rounding.HALF_EVEN)));
        assertEquals(Fixed.fromInt(1L << 32), Fixed.fromDouble(Math.scalb(1.0, 32), Rounding.HALF_EVEN));
        assertEquals(100.25, f(100, 250_000_000).toDouble());
        assertEquals(123456.789, Fixed.fromDouble(123456.789, Rounding.HALF_EVEN).toDouble());
    }

    @Test
    void doubleBoundaryAgreesWithBigDecimalOnRandomDoubles() {
        Random rnd = new Random(5);
        for (int i = 0; i < 100_000; i++) {
            double x = (rnd.nextDouble() - 0.5) * Math.scalb(1.0, rnd.nextInt(40) - 20);
            BigDecimal exact = new BigDecimal(x);
            for (Rounding m : Rounding.values()) {
                long want = exact.setScale(9, RoundingMode.valueOf(m.name())).unscaledValue().longValueExact();
                assertEquals(want, Fixed.fromDouble(x, m).raw(), () -> x + " " + m);
            }
        }
    }

    @Test
    void partsSignAndPredicates() {
        Fixed x = f(-3, 250_000_000);
        assertEquals(-3, x.intPart());
        assertEquals(-250_000_000, x.fracPart());
        assertTrue(x.isNegative());
        assertFalse(x.isPositive());
        assertFalse(x.isZero());
        assertEquals(-1, x.signum());
        assertEquals(0, Fixed.ZERO.signum());
        assertTrue(Fixed.ZERO.isZero());
        assertTrue(Fixed.ONE.isPositive());
    }

    @Test
    void toStringIsTheShortestExactDecimal() {
        assertEquals("100.5", f(100, 500_000_000).toString());
        assertEquals("100", f(100, 0).toString());
        assertEquals("0.000000001", Fixed.fromRaw(1).toString());
        assertEquals("-0.000000001", Fixed.fromRaw(-1).toString());
        assertEquals("-1.25", f(-1, 250_000_000).toString());
        assertEquals("0", Fixed.ZERO.toString());
        assertEquals("9223372036.854775807", Fixed.MAX.toString());
        assertEquals("-9223372036.854775808", Fixed.MIN.toString());
        StringBuilder sb = new StringBuilder("px=");
        f(1, 100_000_000).appendTo(sb);
        assertEquals("px=1.1", sb.toString());
    }

    @Test
    void stressRandomProductsMatchA128BitOracle() {
        Random rnd = new Random(99);
        for (int i = 0; i < 200_000; i++) {
            Fixed a = Fixed.fromRaw(rnd.nextLong() % 1_000_000_000_000L);
            Fixed b = Fixed.fromRaw(rnd.nextLong() % 10_000_000_000L);
            BigInteger exact = BigInteger.valueOf(a.raw()).multiply(BigInteger.valueOf(b.raw()));
            BigInteger[] qr = exact.divideAndRemainder(BigInteger.valueOf(Fixed.SCALE));
            BigInteger floor = qr[1].signum() < 0 ? qr[0].subtract(BigInteger.ONE) : qr[0];
            assertEquals(floor.longValueExact(), a.mul(b, Rounding.FLOOR).raw(), () -> a + " * " + b);
            assertEquals(a, a.add(b).sub(b));
            if (!b.isZero()) {
                Fixed q = a.div(b, Rounding.DOWN);
                Fixed r = a.sub(q.mul(b, Rounding.DOWN));
                assertTrue(Math.abs(r.raw()) <= Math.abs(b.raw()) + 1, () -> a + " / " + b);
            }
        }
    }
}
