package com.submillisecond.recipes.ts;

import java.util.Comparator;

/**
 * Numeric surface gate. A generic {@code TsSeries<T>} lights up
 * {@code min} / {@code max} / {@code sum} / {@code mean} and their ranged
 * variants when handed a {@code TsNumeric<T>} operator bundle. Non-numeric
 * value types (Ohlc, Curve, schemaless) still get the full time-query surface;
 * extract a scalar field first to aggregate.
 *
 * <p>Java has no zero-cost numeric trait the way Rust does, so the numeric
 * operations are supplied as an explicit operator bundle. {@link TsSeriesD}
 * and {@link TsSeriesL} sidestep this on the unboxed fast path - their
 * aggregates run on primitive {@code double[]} / {@code long[]} columns.
 */
public interface TsNumeric<T> extends Comparator<T> {

    T zero();

    T add(T a, T b);

    double toDouble(T v);

    TsNumeric<Double> DOUBLE = new TsNumeric<>() {
        @Override
        public Double zero() {
            return 0.0;
        }

        @Override
        public Double add(Double a, Double b) {
            return a + b;
        }

        @Override
        public double toDouble(Double v) {
            return v;
        }

        @Override
        public int compare(Double a, Double b) {
            return Double.compare(a, b);
        }
    };

    TsNumeric<Long> LONG = new TsNumeric<>() {
        @Override
        public Long zero() {
            return 0L;
        }

        @Override
        public Long add(Long a, Long b) {
            return a + b;
        }

        @Override
        public double toDouble(Long v) {
            return (double) v;
        }

        @Override
        public int compare(Long a, Long b) {
            return Long.compare(a, b);
        }
    };
}
