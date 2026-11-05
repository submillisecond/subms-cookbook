package com.submillisecond.recipes.fixedpoint;

import com.submillisecond.recipes.fixedpoint.features.Currency;
import com.submillisecond.recipes.fixedpoint.features.Money;
import com.submillisecond.recipes.fixedpoint.features.Text;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        Fixed px = Fixed.fromParts(100, 250_000_000);     // 100.25
        Fixed qty = Fixed.fromInt(300);
        Fixed tick = Fixed.fromParts(0, 10_000_000);      // 0.01

        Fixed notional = px.mul(qty, Rounding.HALF_EVEN);
        assertEquals(Fixed.fromInt(30_075), notional);
        assertTrue(px.isOnTick(tick));

        Fixed offGrid = Fixed.fromParts(100, 254_000_000);
        assertEquals(px, offGrid.roundToTick(tick, Rounding.HALF_EVEN));
        assertEquals("30075", notional.toString());
        // quickstart:end
    }

    private record Fill(Fixed px, Fixed qty) {}

    private static List<Fill> fills(long seed, int count) {
        SampleApp.Fills f = new SampleApp.Fills(seed, count);
        List<Fill> out = new ArrayList<>(count);
        while (f.hasNext()) {
            f.advance();
            out.add(new Fill(f.px, f.qty));
        }
        return out;
    }

    @Test
    void baseLedgerNetsExactlyAndIndependentOfFillOrder() {
        Fixed tick = Fixed.fromRaw(10_000_000L);
        List<Fill> fills = fills(7, 50_000);
        List<Fill> reversed = new ArrayList<>(fills);
        Collections.reverse(reversed);

        Fixed[] forward = net(fills, tick);
        Fixed[] backward = net(reversed, tick);
        assertEquals(forward[0], backward[0]);
        assertEquals(forward[1], backward[1]);
        assertTrue(forward[1].isOnTick(tick), "cash of on-grid fills is on grid");

        double cashF = 0.0;
        for (Fill f : fills) {
            cashF -= f.px.toDouble() * f.qty.toDouble();
        }
        double cashR = 0.0;
        for (Fill f : reversed) {
            cashR -= f.px.toDouble() * f.qty.toDouble();
        }
        assertNotEquals(cashF, cashR, "the double ledger depends on fill order");
    }

    private static Fixed[] net(List<Fill> fills, Fixed tick) {
        Fixed position = Fixed.ZERO;
        Fixed cash = Fixed.ZERO;
        for (Fill f : fills) {
            assertTrue(f.px.isOnTick(tick));
            position = position.add(f.qty);
            cash = cash.sub(f.px.mul(f.qty, Rounding.HALF_EVEN));
        }
        return new Fixed[] {position, cash};
    }

    @Test
    void textSectionAcceptsTheWireShapesAndRefusesExcessPrecision() {
        assertEquals(100_250_000_000L, Text.parse("100.25").raw());
        assertEquals(100_250_000_000L, Text.parse("0100.250").raw());
        assertEquals(FixedException.Kind.INVALID_TEXT,
                assertThrows(FixedException.class, () -> Text.parse("1e3")).kind());
        assertEquals(FixedException.Kind.TOO_PRECISE,
                assertThrows(FixedException.class, () -> Text.parse("100.0000000001")).kind());
        Fixed total = Fixed.ZERO;
        for (Fill f : fills(7, 1_000)) {
            total = total.add(f.px.mul(f.qty, Rounding.HALF_EVEN));
        }
        byte[] buf = new byte[32];
        int n2 = Text.writeTo(total, buf, 0, 2);
        String two = new String(buf, 0, n2, StandardCharsets.US_ASCII);
        int n9 = Text.writeTo(total, buf, 0, 9);
        String nine = new String(buf, 0, n9, StandardCharsets.US_ASCII);
        assertTrue(nine.startsWith(two), nine + " starts with " + two);
        assertEquals(total, Text.parse(nine));
    }

    @Test
    void moneySectionRefusesCrossCurrencyAndSettlesPerMinorUnit() {
        Money usd = new Money(Fixed.fromParts(1_250_000, 375_000_000), Currency.USD);
        Money eur = new Money(Fixed.fromParts(-840_000, 125_000_000), Currency.EUR);
        assertEquals(FixedException.Kind.CURRENCY_MISMATCH,
                assertThrows(FixedException.class, () -> usd.add(eur)).kind());
        Fixed rate = Fixed.fromParts(1, 84_500_000);
        Money net = usd.add(eur.convert(rate, Currency.USD, Rounding.HALF_EVEN));
        assertEquals(Currency.USD, net.currency());
        assertEquals(1_250_000_375_000_000L - 910_980_135_562_500L, net.amount().raw());
        assertFalse(net.isSettleable());
        assertTrue(net.roundToMinor(Rounding.HALF_EVEN).isSettleable());
        Money jpy = new Money(Fixed.fromParts(12_345_678, 900_000_000), Currency.JPY);
        assertEquals(Fixed.fromInt(12_345_679), jpy.roundToMinor(Rounding.HALF_EVEN).amount());
    }

    @Test
    void wideSectionHoldsASessionTheBaseTypeCannot() {
        Fixed128 gross = Fixed128.ZERO;
        Fixed narrow = Fixed.ZERO;
        long overflowedAt = -1;
        long count = 0;
        SampleApp.Fills fills = new SampleApp.Fills(11, 1_000_000);
        while (fills.hasNext()) {
            fills.advance();
            Fixed lots = fills.qty.abs();
            gross = gross.addProduct(fills.px, lots);
            count++;
            if (overflowedAt < 0) {
                try {
                    narrow = narrow.add(fills.px.mul(lots, Rounding.HALF_EVEN));
                } catch (FixedException e) {
                    overflowedAt = count;
                }
            }
        }
        assertFalse(gross.isZero());
        assertTrue(overflowedAt > 0, "a million fills around 100 x 2500 lots passes 9.2 billion");
        assertEquals(FixedException.Kind.OVERFLOW,
                assertThrows(FixedException.class, () -> Fixed128.ZERO.add(gross()).toFixed(Rounding.HALF_EVEN)).kind());
        Fixed avg = gross.divInt(1_000_000, Rounding.HALF_EVEN).toFixed(Rounding.HALF_EVEN);
        assertTrue(avg.compareTo(Fixed.fromInt(200_000)) > 0 && avg.compareTo(Fixed.fromInt(300_000)) < 0);
    }

    private static Fixed128 gross() {
        Fixed128 gross = Fixed128.ZERO;
        SampleApp.Fills fills = new SampleApp.Fills(11, 1_000_000);
        while (fills.hasNext()) {
            fills.advance();
            gross = gross.addProduct(fills.px, fills.qty.abs());
        }
        return gross;
    }

    @Test
    void mainRunsEverySectionAndPrintsDeterministically() {
        PrintStream orig = System.out;
        ByteArrayOutputStream a = new ByteArrayOutputStream();
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        try {
            System.setOut(new PrintStream(a, true, StandardCharsets.UTF_8));
            SampleApp.main(new String[0]);
            System.setOut(new PrintStream(b, true, StandardCharsets.UTF_8));
            SampleApp.main(new String[0]);
        } finally {
            System.setOut(orig);
        }
        String out = a.toString(StandardCharsets.UTF_8);
        assertEquals(out, b.toString(StandardCharsets.UTF_8));
        assertTrue(out.contains("== base: netting a day of fills =="));
        assertTrue(out.contains("== text: the wire boundary =="));
        assertTrue(out.contains("== money: a two-currency book =="));
        assertTrue(out.contains("== wide: a session's notional =="));
        assertTrue(out.contains("refused, currencies differ"));
        assertTrue(out.contains("a 64-bit running sum overflowed at fill"));
    }
}
