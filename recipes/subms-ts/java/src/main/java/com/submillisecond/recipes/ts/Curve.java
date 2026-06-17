package com.submillisecond.recipes.ts;

import java.util.Arrays;

/**
 * A term structure / yield curve at one instant: parallel {@code axis} (tenor
 * / strike) and {@code values} columns. A curve time series is
 * {@code TsSeries<Curve>} - each point is a whole curve snapshot.
 */
public final class Curve implements TsValueKind {

    private final double[] axis;
    private final double[] values;

    public Curve(double[] axis, double[] values) {
        this.axis = axis.clone();
        this.values = values.clone();
    }

    public static Curve of(double[] axis, double[] values) {
        return new Curve(axis, values);
    }

    public double[] axis() {
        return axis.clone();
    }

    public double[] values() {
        return values.clone();
    }

    @Override
    public boolean tsIsPresent() {
        for (double x : axis) {
            if (!Double.isFinite(x)) return false;
        }
        for (double v : values) {
            if (!Double.isFinite(v)) return false;
        }
        return true;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Curve c)) return false;
        return Arrays.equals(axis, c.axis) && Arrays.equals(values, c.values);
    }

    @Override
    public int hashCode() {
        return 31 * Arrays.hashCode(axis) + Arrays.hashCode(values);
    }

    @Override
    public String toString() {
        return "Curve{axis=" + Arrays.toString(axis) + ", values=" + Arrays.toString(values) + "}";
    }
}
