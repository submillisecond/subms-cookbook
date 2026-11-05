package com.submillisecond.recipes.fixedpoint;

/** How a quotient that does not land on the grid is resolved. */
public enum Rounding {
    /** Round to the nearest; on a tie, to the even neighbour. */
    HALF_EVEN,
    /** Round to the nearest; on a tie, away from zero. */
    HALF_UP,
    /** Round to the nearest; on a tie, toward zero. */
    HALF_DOWN,
    /** Truncate toward zero. */
    DOWN,
    /** Away from zero. */
    UP,
    /** Toward negative infinity. */
    FLOOR,
    /** Toward positive infinity. */
    CEILING;

    /** Whether a truncated quotient moves one step away from zero. */
    boolean away(boolean negative, int half, boolean quotientOdd) {
        return switch (this) {
            case DOWN -> false;
            case UP -> true;
            case FLOOR -> negative;
            case CEILING -> !negative;
            case HALF_UP -> half >= 0;
            case HALF_DOWN -> half > 0;
            case HALF_EVEN -> half > 0 || (half == 0 && quotientOdd);
        };
    }

    /** {@code n / d} rounded under this mode, in 64 bits. */
    long div(long n, long d) {
        long q = n / d;
        long r = n % d;
        if (r == 0) {
            return q;
        }
        boolean negative = (n < 0) != (d < 0);
        // |2r| against |d|, unsigned: |r| < |d| <= 2^63 so 2|r| fits an unsigned long.
        long twice = Math.abs(r) << 1;
        int half = Long.compareUnsigned(twice, Math.abs(d));
        if (away(negative, half, (q & 1) != 0)) {
            return negative ? q - 1 : q + 1;
        }
        return q;
    }
}
