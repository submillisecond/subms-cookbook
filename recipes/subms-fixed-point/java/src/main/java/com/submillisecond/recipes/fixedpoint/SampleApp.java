package com.submillisecond.recipes.fixedpoint;

import com.submillisecond.recipes.fixedpoint.features.Currency;
import com.submillisecond.recipes.fixedpoint.features.Money;
import com.submillisecond.recipes.fixedpoint.features.Text;

import java.nio.charset.StandardCharsets;
import java.util.Locale;

/** Sample app: a tour of {@code subms-fixed-point}, base API first, then each optional capability. */
public final class SampleApp {

    /** Deterministic fill stream: a buy or sell of 100-5000 lots at a price on the one-cent grid around 100. */
    static final class Fills {
        private long state;
        private int left;

        Fills(long seed, int count) {
            state = seed * 0x9E3779B97F4A7C15L + 1;
            left = count;
        }

        private long next() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return state >>> 11;
        }

        boolean hasNext() {
            return left > 0;
        }

        /** Price of the next fill; call {@link #qty()} right after. */
        Fixed px;
        Fixed qty;

        void advance() {
            left--;
            long cents = 9_950 + Long.remainderUnsigned(next(), 101);
            px = Fixed.fromRaw(cents * 10_000_000L);
            long lots = 100 + Long.remainderUnsigned(next(), 4_901);
            long signed = Long.remainderUnsigned(next(), 2) == 0 ? lots : -lots;
            qty = Fixed.fromInt(signed);
        }
    }

    public static void main(String[] args) {
        baseFillLedger();
        textWireBoundary();
        moneyTwoCurrencyBook();
        wideSessionNotional();
    }

    /** Base API: net 50,000 fills into position and cash. */
    static void baseFillLedger() {
        System.out.println("== base: netting a day of fills ==");
        Fixed tick = Fixed.fromRaw(10_000_000L);

        Fixed position = Fixed.ZERO;
        Fixed cash = Fixed.ZERO;
        double positionF = 0.0;
        double cashF = 0.0;
        int offGrid = 0;

        Fills fills = new Fills(7, 50_000);
        while (fills.hasNext()) {
            fills.advance();
            if (!fills.px.isOnTick(tick)) {
                offGrid++;
            }
            position = position.add(fills.qty);
            cash = cash.sub(fills.px.mul(fills.qty, Rounding.HALF_EVEN));
            positionF += fills.qty.toDouble();
            cashF -= fills.px.toDouble() * fills.qty.toDouble();
        }

        System.out.println("  fills 50000, off-grid prices " + offGrid);
        System.out.println("  position " + position + " lots, cash " + cash);
        System.out.println(String.format(Locale.ROOT, "  f64 ledger position %.1f, cash %.9f", positionF, cashF));
        double drift = cash.toDouble() - cashF;
        System.out.println(String.format(Locale.ROOT,
                "  f64 drift vs exact: %.9f (sign and size depend on fill order)", drift));

        Fixed mark = Fixed.fromParts(100, 10_000_000);
        Fixed mtm = position.mul(mark, Rounding.HALF_EVEN);
        Fixed pnl = cash.add(mtm);
        System.out.println("  mark " + mark + ", position value " + mtm + ", pnl " + pnl);

        String avg;
        try {
            avg = cash.neg().div(position, Rounding.HALF_EVEN).toString();
        } catch (FixedException e) {
            avg = "n/a (flat)";
        }
        System.out.println("  average price " + avg);
        System.out.println("  rounded to the tick: "
                + cash.roundToTick(tick, Rounding.HALF_EVEN) + " (half-even) "
                + cash.roundToTick(tick, Rounding.HALF_UP) + " (half-up) "
                + cash.roundToTick(tick, Rounding.FLOOR) + " (floor)");
    }

    /** text: prices come off the wire as ASCII and go back to the clearer as ASCII at a fixed number of decimals. */
    static void textWireBoundary() {
        System.out.println("== text: the wire boundary ==");
        String[] wire = {"100.25", "99.9", "-0.000000001", "0100.250", "1e3", "100.0000000001"};
        for (String w : wire) {
            String line;
            try {
                line = String.format(Locale.ROOT, "  %16s -> %s", w, Text.parse(w));
            } catch (FixedException e) {
                line = e.kind() == FixedException.Kind.TOO_PRECISE
                        ? String.format(Locale.ROOT, "  %16s -> rejected, finer than 9 decimals", w)
                        : String.format(Locale.ROOT, "  %16s -> rejected, %s", w, e.getMessage());
            }
            System.out.println(line);
        }

        Fixed total = Fixed.ZERO;
        Fills fills = new Fills(7, 1_000);
        while (fills.hasNext()) {
            fills.advance();
            total = total.add(fills.px.mul(fills.qty, Rounding.HALF_EVEN));
        }
        byte[] buf = new byte[32];
        int n = Text.writeTo(total, buf, 0, 2);
        System.out.println("  clearer file, 2dp: " + new String(buf, 0, n, StandardCharsets.US_ASCII));
        n = Text.writeTo(total, buf, 0, 9);
        System.out.println("  audit file, 9dp:   " + new String(buf, 0, n, StandardCharsets.US_ASCII));
        n = Text.writeTo(total.round(2, Rounding.HALF_EVEN), buf, 0, 2);
        System.out.println("  rounded first:     " + new String(buf, 0, n, StandardCharsets.US_ASCII));
    }

    /** money: a book with a USD leg and a EUR leg. */
    static void moneyTwoCurrencyBook() {
        System.out.println("== money: a two-currency book ==");
        Money usd = new Money(Fixed.fromParts(1_250_000, 375_000_000), Currency.USD);
        Money eur = new Money(Fixed.fromParts(-840_000, 125_000_000), Currency.EUR);
        Money jpy = new Money(Fixed.fromParts(12_345_678, 900_000_000), Currency.JPY);

        try {
            Money bad = usd.add(eur);
            System.out.println("  unexpected " + bad);
        } catch (FixedException e) {
            System.out.println("  " + usd + " + " + eur + ": refused, currencies differ");
        }
        Fixed rate = Fixed.fromParts(1, 84_500_000);
        Money eurInUsd = eur.convert(rate, Currency.USD, Rounding.HALF_EVEN);
        Money net = usd.add(eurInUsd);
        System.out.println("  EUR leg at " + rate + ": " + eurInUsd);
        System.out.println("  net USD: " + net);
        System.out.println("  settle: "
                + net.roundToMinor(Rounding.HALF_EVEN) + " / "
                + jpy.roundToMinor(Rounding.HALF_EVEN) + " / "
                + new Money(Fixed.fromParts(10, 123_456_789), Currency.BHD).roundToMinor(Rounding.HALF_EVEN));
        System.out.println("  settleable as quoted: USD " + net.isSettleable() + " JPY " + jpy.isSettleable());
    }

    /** wide: a session's notional on one liquid book passes the 9.2 billion the base type holds. */
    static void wideSessionNotional() {
        System.out.println("== wide: a session's notional ==");
        Fixed128 gross = Fixed128.ZERO;
        long count = 0;
        long overflowedAt = -1;
        Fixed narrowSum = Fixed.ZERO;

        Fills fills = new Fills(11, 1_000_000);
        while (fills.hasNext()) {
            fills.advance();
            Fixed lots = fills.qty.abs();
            gross = gross.addProduct(fills.px, lots);
            count++;
            if (overflowedAt < 0) {
                try {
                    narrowSum = narrowSum.add(fills.px.mul(lots, Rounding.HALF_EVEN));
                } catch (FixedException e) {
                    overflowedAt = count;
                }
            }
        }

        System.out.println("  fills " + count);
        System.out.println("  gross notional (exact, 18dp): " + gross);
        String asFixed;
        try {
            asFixed = gross.toFixed(Rounding.HALF_EVEN).toString();
        } catch (FixedException e) {
            asFixed = "does not fit (" + e.getMessage() + ")";
        }
        System.out.println("  as Fixed: " + asFixed);
        if (overflowedAt >= 0) {
            System.out.println("  a 64-bit running sum overflowed at fill " + overflowedAt);
        } else {
            System.out.println("  a 64-bit running sum held: " + narrowSum);
        }
        Fixed avg = gross.divInt(count, Rounding.HALF_EVEN).toFixed(Rounding.HALF_EVEN);
        System.out.println("  average notional per fill " + avg);
    }
}
