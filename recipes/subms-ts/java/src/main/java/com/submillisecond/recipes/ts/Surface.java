package com.submillisecond.recipes.ts;

import java.util.Arrays;

/**
 * A surface (e.g. an implied-vol surface) at one instant: an
 * {@code axisX} x {@code axisY} grid of {@code values}. A surface time series
 * is {@code TsSeries<Surface>}.
 */
public final class Surface implements TsValueKind {

    private final double[] axisX;
    private final double[] axisY;
    private final double[][] values;

    public Surface(double[] axisX, double[] axisY, double[][] values) {
        this.axisX = axisX.clone();
        this.axisY = axisY.clone();
        this.values = new double[values.length][];
        for (int i = 0; i < values.length; i++) {
            this.values[i] = values[i].clone();
        }
    }

    public static Surface of(double[] axisX, double[] axisY, double[][] values) {
        return new Surface(axisX, axisY, values);
    }

    public double[] axisX() {
        return axisX.clone();
    }

    public double[] axisY() {
        return axisY.clone();
    }

    public double[][] values() {
        double[][] out = new double[values.length][];
        for (int i = 0; i < values.length; i++) {
            out[i] = values[i].clone();
        }
        return out;
    }

    @Override
    public boolean tsIsPresent() {
        for (double x : axisX) {
            if (!Double.isFinite(x)) return false;
        }
        for (double y : axisY) {
            if (!Double.isFinite(y)) return false;
        }
        for (double[] row : values) {
            for (double v : row) {
                if (!Double.isFinite(v)) return false;
            }
        }
        return true;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Surface s)) return false;
        return Arrays.equals(axisX, s.axisX)
                && Arrays.equals(axisY, s.axisY)
                && Arrays.deepEquals(values, s.values);
    }

    @Override
    public int hashCode() {
        return 31 * (31 * Arrays.hashCode(axisX) + Arrays.hashCode(axisY)) + Arrays.deepHashCode(values);
    }
}
