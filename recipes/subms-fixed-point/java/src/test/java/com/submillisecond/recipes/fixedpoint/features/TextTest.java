package com.submillisecond.recipes.fixedpoint.features;

import com.submillisecond.recipes.fixedpoint.Fixed;
import com.submillisecond.recipes.fixedpoint.FixedException;
import org.junit.jupiter.api.Test;

import java.nio.charset.StandardCharsets;
import java.util.Random;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TextTest {

    private static FixedException.Kind kind(Runnable r) {
        return assertThrows(FixedException.class, r::run).kind();
    }

    private static byte[] ascii(String s) {
        return s.getBytes(StandardCharsets.US_ASCII);
    }

    @Test
    void parsesTheCommonShapes() {
        assertEquals(100_250_000_000L, Text.parse("100.25").raw());
        assertEquals(-100_250_000_000L, Text.parse("-100.25").raw());
        assertEquals(7 * Fixed.SCALE, Text.parse("+7").raw());
        assertEquals(100_250_000_000L, Text.parse("0100.250").raw());
        assertEquals(500_000_000L, Text.parse(".5").raw());
        assertEquals(5 * Fixed.SCALE, Text.parse("5.").raw());
        assertEquals(1, Text.parse("0.000000001").raw());
        assertEquals(-1, Text.parse("-0.000000001").raw());
        assertEquals(Fixed.ZERO, Text.parse("0"));
        assertEquals(Fixed.ZERO, Text.parse("-0"));
        assertEquals(12_500_000_000L, Text.parse("px=12.5;", 3, 7).raw());
        assertEquals(12_500_000_000L, Text.parseBytes(ascii("12.5")).raw());
        assertEquals(-12_500_000_000L, Text.parseBytes(ascii("xx-12.5yy"), 2, 7).raw());
    }

    @Test
    void rejectsWhatIsNotAPlainDecimal() {
        for (String bad : new String[] {"", "-", "+", ".", "1e3", " 1", "1 ", "1,000", "1.2.3", "abc", "--1", "0x10"}) {
            assertEquals(FixedException.Kind.INVALID_TEXT, kind(() -> Text.parse(bad)), bad);
            assertEquals(FixedException.Kind.INVALID_TEXT, kind(() -> Text.parseBytes(ascii(bad))), bad);
        }
    }

    @Test
    void tenFractionalDigitsIsTooPreciseEvenWhenZero() {
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Text.parse("1.0000000000")));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Text.parse("1.0000000001")));
        assertEquals(Fixed.ONE, Text.parse("1.000000000"));
    }

    @Test
    void integerOverflowIsReportedNotWrapped() {
        assertEquals(Fixed.MAX, Text.parse("9223372036.854775807"));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Text.parse("9223372036.854775808")));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Text.parse("9223372037")));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> Text.parse("99999999999999999999")));
        assertEquals(-Long.MAX_VALUE, Text.parse("-9223372036.854775807").raw());
    }

    @Test
    void writeToFixesTheDecimalCountAndTruncates() {
        Fixed x = Fixed.fromParts(100, 256_789_000);
        byte[] buf = new byte[32];
        int n = Text.writeTo(x, buf, 0, 2);
        assertArrayEquals(ascii("100.25"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(x, buf, 0, 0);
        assertArrayEquals(ascii("100"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(x, buf, 0, 9);
        assertArrayEquals(ascii("100.256789000"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(Fixed.fromRaw(-1), buf, 0, 9);
        assertArrayEquals(ascii("-0.000000001"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(Fixed.fromRaw(-1), buf, 0, 2);
        assertArrayEquals(ascii("-0.00"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(Fixed.ZERO, buf, 0, 4);
        assertArrayEquals(ascii("0.0000"), java.util.Arrays.copyOf(buf, n));
        n = Text.writeTo(Fixed.MIN, buf, 0, 9);
        assertArrayEquals(ascii("-9223372036.854775808"), java.util.Arrays.copyOf(buf, n));
        assertTrue(n <= Text.MAX_TEXT_LEN);
        n = Text.writeTo(Fixed.fromParts(1, 500_000_000), buf, 5, 1);
        assertEquals(3, n);
        assertArrayEquals(ascii("1.5"), java.util.Arrays.copyOfRange(buf, 5, 8));
    }

    @Test
    void writeToReportsAShortBufferAndTooManyDecimals() {
        Fixed x = Fixed.fromParts(100, 250_000_000);
        assertEquals(FixedException.Kind.BUFFER_TOO_SMALL, kind(() -> Text.writeTo(x, new byte[5], 0, 2)));
        assertEquals(FixedException.Kind.BUFFER_TOO_SMALL, kind(() -> Text.writeTo(x, new byte[8], 4, 2)));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Text.writeTo(x, new byte[32], 0, 10)));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Text.writeTo(x, new byte[32], 0, -1)));
    }

    @Test
    void stringConvenienceMatchesTheBufferForm() {
        Fixed x = Fixed.fromParts(-3, 5_000_000);
        assertEquals("-3.005", Text.toStringDecimals(x, 3));
        assertEquals("-3", Text.toStringDecimals(x, 0));
        assertEquals(FixedException.Kind.TOO_PRECISE, kind(() -> Text.toStringDecimals(x, 12)));
    }

    @Test
    void roundTripIsIdentityOnRandomValues() {
        Random rnd = new Random(3);
        byte[] buf = new byte[32];
        for (int i = 0; i < 100_000; i++) {
            Fixed x = Fixed.fromRaw(rnd.nextLong() / 3);
            int n = Text.writeTo(x, buf, 0, 9);
            assertEquals(x, Text.parseBytes(buf, 0, n));
            assertEquals(x, Text.parse(x.toString()));
        }
    }
}
