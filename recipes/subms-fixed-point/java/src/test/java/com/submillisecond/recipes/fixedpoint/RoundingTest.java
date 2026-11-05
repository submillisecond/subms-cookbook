package com.submillisecond.recipes.fixedpoint;

import org.junit.jupiter.api.Test;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertEquals;

final class RoundingTest {

    private static RoundingMode javaMode(Rounding r) {
        return RoundingMode.valueOf(r.name());
    }

    @Test
    void exactQuotientIsUntouchedByEveryMode() {
        for (Rounding m : Rounding.values()) {
            assertEquals(10, m.div(100, 10), m.name());
            assertEquals(-10, m.div(-100, 10), m.name());
            assertEquals(0, m.div(0, 7), m.name());
        }
    }

    @Test
    void tiesFollowJavaRoundingModeSemantics() {
        long[][] want = {
            {2, -2, 4, -4}, // HALF_EVEN
            {3, -3, 4, -4}, // HALF_UP
            {2, -2, 3, -3}, // HALF_DOWN
            {2, -2, 3, -3}, // DOWN
            {3, -3, 4, -4}, // UP
            {2, -3, 3, -4}, // FLOOR
            {3, -2, 4, -3}, // CEILING
        };
        Rounding[] modes = Rounding.values();
        for (int i = 0; i < modes.length; i++) {
            Rounding m = modes[i];
            assertEquals(want[i][0], m.div(25, 10), m + " 2.5");
            assertEquals(want[i][1], m.div(-25, 10), m + " -2.5");
            assertEquals(want[i][2], m.div(35, 10), m + " 3.5");
            assertEquals(want[i][3], m.div(-35, 10), m + " -3.5");
        }
    }

    @Test
    void negativeDivisorFollowsTheMathematicalQuotient() {
        assertEquals(-4, Rounding.FLOOR.div(7, -2));
        assertEquals(-3, Rounding.CEILING.div(7, -2));
        assertEquals(-4, Rounding.HALF_EVEN.div(7, -2));
        assertEquals(-2, Rounding.HALF_EVEN.div(5, -2));
        assertEquals(-3, Rounding.HALF_UP.div(5, -2));
    }

    @Test
    void halfEvenAlternatesOnARunOfTies() {
        long[] got = new long[8];
        for (int k = 0; k < 8; k++) {
            got[k] = Rounding.HALF_EVEN.div(2L * k + 1, 2);
        }
        assertEquals("[0, 2, 2, 4, 4, 6, 6, 8]", java.util.Arrays.toString(got));
    }

    @Test
    void extremeMagnitudesDoNotOverflowTheTieTest() {
        assertEquals(Long.MAX_VALUE / 2 + 1, Rounding.HALF_EVEN.div(Long.MAX_VALUE, 2));
        assertEquals((Long.MIN_VALUE + 1) / 3, Rounding.DOWN.div(Long.MIN_VALUE + 1, 3));
        assertEquals(1, Rounding.HALF_UP.div(Long.MIN_VALUE + 1, Long.MIN_VALUE + 1));
        assertEquals(-1, Rounding.HALF_UP.div(Long.MIN_VALUE + 1, Long.MAX_VALUE));
        assertEquals(1, Rounding.HALF_UP.div(Long.MIN_VALUE, Long.MIN_VALUE));
    }

    @Test
    void everyModeAgreesWithBigDecimalOnRandomQuotients() {
        Random rnd = new Random(42);
        for (int i = 0; i < 200_000; i++) {
            long n = rnd.nextLong() >> rnd.nextInt(40);
            long d = (rnd.nextLong() >> rnd.nextInt(50));
            if (d == 0 || (n == Long.MIN_VALUE && d == -1)) {
                continue;
            }
            for (Rounding m : Rounding.values()) {
                long want = BigDecimal.valueOf(n)
                        .divide(BigDecimal.valueOf(d), 0, javaMode(m))
                        .longValueExact();
                assertEquals(want, m.div(n, d), () -> n + " / " + d + " " + m);
            }
        }
    }
}
