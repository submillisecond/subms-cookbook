package com.submillisecond.recipes.ts;

import java.util.Optional;

/**
 * A type-erased column: a typed time series. The variant carries the element
 * type, so a scan reaches the underlying homogeneous series unboxed - an f64
 * column is a {@link TsSeriesD} (primitive {@code double[]}), not a list of
 * boxed cells. A stored column never holds nulls (the series rejects them on
 * push); nulls surface only in {@link TsDataFrame#aligned} as gap rows.
 */
public sealed interface TsColumn
        permits TsColumn.F64, TsColumn.I64, TsColumn.Bool, TsColumn.Str, TsColumn.Value {

    TsDataType dataType();

    int len();

    default boolean isEmpty() {
        return len() == 0;
    }

    /** Dynamic single-cell read at an exact ts, boxed into {@link TsValue}. */
    Optional<TsValue> get(long ts);

    /** The f64 series, present only for an {@link F64} column. */
    default Optional<TsSeriesD> asF64() {
        return Optional.empty();
    }

    /** The i64 series, present only for an {@link I64} column. */
    default Optional<TsSeriesL> asI64() {
        return Optional.empty();
    }

    /** The boolean series, present only for a {@link Bool} column. */
    default Optional<TsSeries<Boolean>> asBool() {
        return Optional.empty();
    }

    /** The string series, present only for a {@link Str} column. */
    default Optional<TsSeries<String>> asStr() {
        return Optional.empty();
    }

    /** The unstructured series, present only for a {@link Value} column. */
    default Optional<TsSeries<TsValue>> asValue() {
        return Optional.empty();
    }

    record F64(TsSeriesD series) implements TsColumn {
        @Override
        public TsDataType dataType() {
            return TsDataType.F64;
        }

        @Override
        public int len() {
            return series.size();
        }

        @Override
        public Optional<TsValue> get(long ts) {
            return series.getAt(ts).map(p -> TsValue.ofDouble(p.value()));
        }

        @Override
        public Optional<TsSeriesD> asF64() {
            return Optional.of(series);
        }
    }

    record I64(TsSeriesL series) implements TsColumn {
        @Override
        public TsDataType dataType() {
            return TsDataType.I64;
        }

        @Override
        public int len() {
            return series.size();
        }

        @Override
        public Optional<TsValue> get(long ts) {
            return series.getAt(ts).map(p -> TsValue.ofLong(p.value()));
        }

        @Override
        public Optional<TsSeriesL> asI64() {
            return Optional.of(series);
        }
    }

    record Bool(TsSeries<Boolean> series) implements TsColumn {
        @Override
        public TsDataType dataType() {
            return TsDataType.BOOL;
        }

        @Override
        public int len() {
            return series.size();
        }

        @Override
        public Optional<TsValue> get(long ts) {
            return series.getAt(ts).map(p -> TsValue.ofBool(p.value()));
        }

        @Override
        public Optional<TsSeries<Boolean>> asBool() {
            return Optional.of(series);
        }
    }

    record Str(TsSeries<String> series) implements TsColumn {
        @Override
        public TsDataType dataType() {
            return TsDataType.STR;
        }

        @Override
        public int len() {
            return series.size();
        }

        @Override
        public Optional<TsValue> get(long ts) {
            return series.getAt(ts).map(p -> TsValue.ofString(p.value()));
        }

        @Override
        public Optional<TsSeries<String>> asStr() {
            return Optional.of(series);
        }
    }

    record Value(TsSeries<TsValue> series) implements TsColumn {
        @Override
        public TsDataType dataType() {
            return TsDataType.VALUE;
        }

        @Override
        public int len() {
            return series.size();
        }

        @Override
        public Optional<TsValue> get(long ts) {
            return series.getAt(ts).map(TsPoint::value);
        }

        @Override
        public Optional<TsSeries<TsValue>> asValue() {
            return Optional.of(series);
        }
    }
}
