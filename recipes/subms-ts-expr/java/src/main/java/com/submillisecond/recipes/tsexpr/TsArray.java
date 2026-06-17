package com.submillisecond.recipes.tsexpr;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsDataType;
import com.submillisecond.recipes.ts.TsValue;

/**
 * The typed, nullable result of evaluating a {@link TsExpr}: one variant per
 * supported element type ({@link TsDataType}), each a dense value buffer paired
 * with an Arrow-style validity bitmap. The two are the same length, indexed by
 * the frame's union-of-timestamps row axis: a cell is meaningful only where
 * {@code valid[i]} is set, and the value under an invalid cell is unspecified
 * (kept at a type default, never relied on).
 *
 * <p>This is the computed-column primitive the rest of the analytical arc
 * rebuilds on. The validity model is for DERIVED nulls - a {@code Col} over a
 * row where the column has a gap, a divide-by-zero, a null propagated through a
 * binary op. It stays distinct from {@code TsSeries}' no-null-on-ingest
 * invariant: a series never stores a null, but aligning several series onto a
 * shared row axis legitimately produces missing cells.
 *
 * <p>Byte-equivalent to the Rust sibling's {@code TsArray} enum: same variants,
 * same surface, modulo case style.
 */
public sealed interface TsArray
        permits TsArray.F64, TsArray.I64, TsArray.Bool, TsArray.Str {

    TsDataType dataType();

    int len();

    default boolean isEmpty() {
        return len() == 0;
    }

    /** The validity bitmap. {@code valid()[i] == false} marks a null at row i. */
    boolean[] valid();

    /** Count of present (non-null) cells. */
    default int validCount() {
        int n = 0;
        for (boolean v : valid()) {
            if (v) n++;
        }
        return n;
    }

    /** The boxed value at {@code i}, or empty when that cell is null. */
    Optional<TsValue> get(int i);

    /** Replace every null cell with {@code fill}; returns a fully-valid array. */
    TsArray fillNull(TsValue fill);

    /** Compact out every null cell; returns a shorter fully-valid array. */
    TsArray dropNulls();

    static boolean[] allTrue(int n) {
        boolean[] v = new boolean[n];
        Arrays.fill(v, true);
        return v;
    }

    record F64(double[] values, boolean[] valid) implements TsArray {
        public F64 {
            if (values.length != valid.length) {
                throw new IllegalArgumentException("TsArray values/valid length mismatch");
            }
        }

        @Override
        public TsDataType dataType() {
            return TsDataType.F64;
        }

        @Override
        public int len() {
            return values.length;
        }

        @Override
        public Optional<TsValue> get(int i) {
            return valid[i] ? Optional.of(TsValue.ofDouble(values[i])) : Optional.empty();
        }

        @Override
        public TsArray fillNull(TsValue fill) {
            double f = switch (fill) {
                case TsValue.F64 v -> v.value();
                case TsValue.I64 v -> (double) v.value();
                default -> Double.NaN;
            };
            double[] out = new double[values.length];
            for (int i = 0; i < values.length; i++) {
                out[i] = valid[i] ? values[i] : f;
            }
            return new F64(out, allTrue(values.length));
        }

        @Override
        public TsArray dropNulls() {
            double[] out = new double[validCount()];
            int j = 0;
            for (int i = 0; i < values.length; i++) {
                if (valid[i]) out[j++] = values[i];
            }
            return new F64(out, allTrue(out.length));
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof F64 a
                    && Arrays.equals(values, a.values)
                    && Arrays.equals(valid, a.valid);
        }

        @Override
        public int hashCode() {
            return 31 * Arrays.hashCode(values) + Arrays.hashCode(valid);
        }
    }

    record I64(long[] values, boolean[] valid) implements TsArray {
        public I64 {
            if (values.length != valid.length) {
                throw new IllegalArgumentException("TsArray values/valid length mismatch");
            }
        }

        @Override
        public TsDataType dataType() {
            return TsDataType.I64;
        }

        @Override
        public int len() {
            return values.length;
        }

        @Override
        public Optional<TsValue> get(int i) {
            return valid[i] ? Optional.of(TsValue.ofLong(values[i])) : Optional.empty();
        }

        @Override
        public TsArray fillNull(TsValue fill) {
            long f = fill instanceof TsValue.I64 v ? v.value() : 0L;
            long[] out = new long[values.length];
            for (int i = 0; i < values.length; i++) {
                out[i] = valid[i] ? values[i] : f;
            }
            return new I64(out, allTrue(values.length));
        }

        @Override
        public TsArray dropNulls() {
            long[] out = new long[validCount()];
            int j = 0;
            for (int i = 0; i < values.length; i++) {
                if (valid[i]) out[j++] = values[i];
            }
            return new I64(out, allTrue(out.length));
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof I64 a
                    && Arrays.equals(values, a.values)
                    && Arrays.equals(valid, a.valid);
        }

        @Override
        public int hashCode() {
            return 31 * Arrays.hashCode(values) + Arrays.hashCode(valid);
        }
    }

    record Bool(boolean[] values, boolean[] valid) implements TsArray {
        public Bool {
            if (values.length != valid.length) {
                throw new IllegalArgumentException("TsArray values/valid length mismatch");
            }
        }

        @Override
        public TsDataType dataType() {
            return TsDataType.BOOL;
        }

        @Override
        public int len() {
            return values.length;
        }

        @Override
        public Optional<TsValue> get(int i) {
            return valid[i] ? Optional.of(TsValue.ofBool(values[i])) : Optional.empty();
        }

        @Override
        public TsArray fillNull(TsValue fill) {
            boolean f = fill instanceof TsValue.Bool v && v.value();
            boolean[] out = new boolean[values.length];
            for (int i = 0; i < values.length; i++) {
                out[i] = valid[i] ? values[i] : f;
            }
            return new Bool(out, allTrue(values.length));
        }

        @Override
        public TsArray dropNulls() {
            boolean[] out = new boolean[validCount()];
            int j = 0;
            for (int i = 0; i < values.length; i++) {
                if (valid[i]) out[j++] = values[i];
            }
            return new Bool(out, allTrue(out.length));
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Bool a
                    && Arrays.equals(values, a.values)
                    && Arrays.equals(valid, a.valid);
        }

        @Override
        public int hashCode() {
            return 31 * Arrays.hashCode(values) + Arrays.hashCode(valid);
        }
    }

    record Str(String[] values, boolean[] valid) implements TsArray {
        public Str {
            if (values.length != valid.length) {
                throw new IllegalArgumentException("TsArray values/valid length mismatch");
            }
        }

        @Override
        public TsDataType dataType() {
            return TsDataType.STR;
        }

        @Override
        public int len() {
            return values.length;
        }

        @Override
        public Optional<TsValue> get(int i) {
            return valid[i] ? Optional.of(TsValue.ofString(values[i])) : Optional.empty();
        }

        @Override
        public TsArray fillNull(TsValue fill) {
            String f = fill instanceof TsValue.Str v ? v.value() : "";
            String[] out = new String[values.length];
            for (int i = 0; i < values.length; i++) {
                out[i] = valid[i] ? values[i] : f;
            }
            return new Str(out, allTrue(values.length));
        }

        @Override
        public TsArray dropNulls() {
            List<String> kept = new ArrayList<>(validCount());
            for (int i = 0; i < values.length; i++) {
                if (valid[i]) kept.add(values[i]);
            }
            return new Str(kept.toArray(new String[0]), allTrue(kept.size()));
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Str a
                    && Arrays.equals(values, a.values)
                    && Arrays.equals(valid, a.valid);
        }

        @Override
        public int hashCode() {
            return 31 * Arrays.hashCode(values) + Arrays.hashCode(valid);
        }
    }
}
