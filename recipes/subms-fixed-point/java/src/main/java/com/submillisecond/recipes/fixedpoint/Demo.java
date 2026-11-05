package com.submillisecond.recipes.fixedpoint;

public final class Demo {
    public static void main(String[] args) {
        Fixed px = Fixed.fromParts(100, 250_000_000);
        Fixed qty = Fixed.fromInt(300);
        Fixed tick = Fixed.fromRaw(10_000_000L);

        Fixed notional = px.mul(qty, Rounding.HALF_EVEN);
        System.out.println(px + " x " + qty + " = " + notional);

        Fixed off = Fixed.fromParts(100, 254_000_000);
        System.out.println(off + " on a " + tick + " grid: "
                + off.roundToTick(tick, Rounding.HALF_EVEN) + " (half-even), "
                + off.roundToTick(tick, Rounding.FLOOR) + " (floor), on grid: "
                + off.isOnTick(tick));

        Fixed tenth = Fixed.fromParts(0, 100_000_000);
        Fixed total = Fixed.ZERO;
        for (int i = 0; i < 10; i++) {
            total = total.add(tenth);
        }
        System.out.println("ten fills of 0.1 sum to " + total + " exactly");
    }
}
