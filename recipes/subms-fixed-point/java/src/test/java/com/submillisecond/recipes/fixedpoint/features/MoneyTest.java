package com.submillisecond.recipes.fixedpoint.features;

import com.submillisecond.recipes.fixedpoint.Fixed;
import com.submillisecond.recipes.fixedpoint.FixedException;
import com.submillisecond.recipes.fixedpoint.Rounding;
import org.junit.jupiter.api.Test;

import java.util.HashSet;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MoneyTest {

    private static Money usd(String s) {
        return new Money(Text.parse(s), Currency.USD);
    }

    private static FixedException.Kind kind(Runnable r) {
        return assertThrows(FixedException.class, r::run).kind();
    }

    @Test
    void currencyCodesValidateNormaliseAndRoundTrip() {
        assertEquals(Currency.USD, Currency.of("usd"));
        assertEquals(Currency.EUR, Currency.of("Eur"));
        assertEquals("USD", Currency.USD.letters());
        assertEquals("USD", Currency.USD.toString());
        for (String bad : new String[] {"US", "USDD", "U$D", "", "123"}) {
            assertEquals(FixedException.Kind.INVALID_CURRENCY, kind(() -> Currency.of(bad)), bad);
        }
        assertEquals(FixedException.Kind.INVALID_CURRENCY, kind(() -> Currency.of(null)));
        assertTrue(Currency.USD.compareTo(Currency.of("XAU")) < 0);
    }

    @Test
    void minorUnitExponentsFollowIso4217() {
        assertEquals(2, Currency.USD.minorUnitDecimals());
        assertEquals(2, Currency.EUR.minorUnitDecimals());
        assertEquals(0, Currency.JPY.minorUnitDecimals());
        assertEquals(0, Currency.KRW.minorUnitDecimals());
        assertEquals(3, Currency.BHD.minorUnitDecimals());
        assertEquals(3, Currency.KWD.minorUnitDecimals());
        assertEquals(2, Currency.of("XYZ").minorUnitDecimals());
    }

    @Test
    void sameCurrencyArithmeticIsExact() {
        Money a = usd("100.25");
        Money b = usd("0.75");
        assertEquals(usd("101"), a.add(b));
        assertEquals(usd("99.5"), a.sub(b));
        assertEquals(Text.parse("100.25"), a.amount());
        assertEquals(Currency.USD, a.currency());
        assertEquals(a, Money.zero(Currency.USD).add(a));
        assertTrue(Money.zero(Currency.JPY).isZero());
        assertTrue(usd("-1").isNegative());
        assertFalse(a.isNegative());
    }

    @Test
    void crossCurrencyArithmeticIsRefused() {
        Money a = usd("100");
        Money e = new Money(Text.parse("100"), Currency.EUR);
        assertEquals(FixedException.Kind.CURRENCY_MISMATCH, kind(() -> a.add(e)));
        assertEquals(FixedException.Kind.CURRENCY_MISMATCH, kind(() -> a.sub(e)));
        assertNotEquals(a, e);
    }

    @Test
    void overflowIsReported() {
        Money big = new Money(Fixed.MAX, Currency.USD);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> big.add(usd("1"))));
        Money small = new Money(Fixed.MIN, Currency.USD);
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> small.sub(usd("1"))));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> big.mul(Text.parse("2"), Rounding.HALF_EVEN)));
        assertEquals(FixedException.Kind.OVERFLOW, kind(() -> big.convert(Text.parse("2"), Currency.EUR, Rounding.HALF_EVEN)));
    }

    @Test
    void scalingAndConversionRoundOnceUnderTheNamedMode() {
        assertEquals(usd("0.125"), usd("1000").mul(Text.parse("0.000125"), Rounding.HALF_EVEN));
        assertEquals(new Money(Text.parse("92.15"), Currency.EUR),
                usd("100").convert(Text.parse("0.9215"), Currency.EUR, Rounding.HALF_EVEN));
        Money third = usd("1").convert(Fixed.fromRaw(333_333_333), Currency.EUR, Rounding.HALF_EVEN);
        assertEquals(333_333_333, third.amount().raw());
    }

    @Test
    void settlementRoundsToTheCurrencyMinorUnit() {
        Money x = usd("10.125");
        assertEquals(usd("10.12"), x.roundToMinor(Rounding.HALF_EVEN));
        assertEquals(usd("10.13"), x.roundToMinor(Rounding.HALF_UP));
        assertFalse(x.isSettleable());
        assertTrue(usd("10.12").isSettleable());
        Money y = new Money(Text.parse("1234.5"), Currency.JPY);
        assertEquals(Text.parse("1234"), y.roundToMinor(Rounding.HALF_EVEN).amount());
        assertEquals(Text.parse("1235"), y.roundToMinor(Rounding.HALF_UP).amount());
        Money b = new Money(Text.parse("1.23456"), Currency.BHD);
        assertEquals(Text.parse("1.235"), b.roundToMinor(Rounding.HALF_EVEN).amount());
        assertTrue(new Money(Text.parse("1.234"), Currency.BHD).isSettleable());
        Money neg = usd("-10.125");
        assertEquals(usd("-10.13"), neg.roundToMinor(Rounding.FLOOR));
        assertEquals(usd("-10.12"), neg.roundToMinor(Rounding.CEILING));
    }

    @Test
    void displayAndHash() {
        assertEquals("100.5 USD", usd("100.5").toString());
        assertEquals("-3 JPY", new Money(Text.parse("-3"), Currency.JPY).toString());
        Set<Money> set = new HashSet<>();
        set.add(usd("1"));
        set.add(usd("1"));
        set.add(usd("2"));
        assertEquals(2, set.size());
    }
}
