package com.submillisecond.recipes.fixedpoint.features;

import com.submillisecond.recipes.fixedpoint.FixedException;

/** A three-letter ISO 4217 code packed into an {@code int}. */
public record Currency(int code) implements Comparable<Currency> {

    public static final Currency USD = ofAscii('U', 'S', 'D');
    public static final Currency EUR = ofAscii('E', 'U', 'R');
    public static final Currency GBP = ofAscii('G', 'B', 'P');
    public static final Currency JPY = ofAscii('J', 'P', 'Y');
    public static final Currency CHF = ofAscii('C', 'H', 'F');
    public static final Currency AUD = ofAscii('A', 'U', 'D');
    public static final Currency CAD = ofAscii('C', 'A', 'D');
    public static final Currency HKD = ofAscii('H', 'K', 'D');
    public static final Currency SGD = ofAscii('S', 'G', 'D');
    public static final Currency CNH = ofAscii('C', 'N', 'H');
    public static final Currency KRW = ofAscii('K', 'R', 'W');
    public static final Currency BHD = ofAscii('B', 'H', 'D');
    public static final Currency KWD = ofAscii('K', 'W', 'D');

    private static Currency ofAscii(char a, char b, char c) {
        return new Currency((a << 16) | (b << 8) | c);
    }

    /** Validate and upper-case a three-letter code. */
    public static Currency of(String code) {
        if (code == null || code.length() != 3) {
            throw new FixedException(FixedException.Kind.INVALID_CURRENCY);
        }
        int packed = 0;
        for (int i = 0; i < 3; i++) {
            char c = code.charAt(i);
            boolean letter = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
            if (!letter) {
                throw new FixedException(FixedException.Kind.INVALID_CURRENCY);
            }
            packed = (packed << 8) | Character.toUpperCase(c);
        }
        return new Currency(packed);
    }

    /** The three ASCII letters. */
    public String letters() {
        return new String(
                new char[] {(char) ((code >> 16) & 0xFF), (char) ((code >> 8) & 0xFF), (char) (code & 0xFF)});
    }

    /** The ISO 4217 minor-unit exponent: 0 for JPY and KRW, 3 for BHD and KWD, 2 for everything else. */
    public int minorUnitDecimals() {
        if (code == JPY.code || code == KRW.code) {
            return 0;
        }
        if (code == BHD.code || code == KWD.code) {
            return 3;
        }
        return 2;
    }

    @Override
    public int compareTo(Currency o) {
        return Integer.compare(code, o.code);
    }

    @Override
    public String toString() {
        return letters();
    }
}
