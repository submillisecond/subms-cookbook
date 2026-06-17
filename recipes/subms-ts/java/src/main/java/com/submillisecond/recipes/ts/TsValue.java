package com.submillisecond.recipes.ts;

import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Schemaless value. The "I don't know the value type yet" path:
 * {@code TsSeries<TsValue>} gets the full time-query surface; the numeric
 * surface stays dark (downcast per point to aggregate). {@link Map} /
 * {@link Array} carry arbitrarily nested JSON-shaped documents.
 */
public sealed interface TsValue extends TsValueKind
        permits TsValue.I64, TsValue.F64, TsValue.Bool, TsValue.Str,
                TsValue.Bytes, TsValue.Null, TsValue.MapVal, TsValue.Array {

    @Override
    default boolean tsIsPresent() {
        // Only a top-level Null is a missing observation. A Null nested inside
        // a Map / Array is the caller's intentional null inside a document.
        return !(this instanceof Null);
    }

    record I64(long value) implements TsValue {}

    record F64(double value) implements TsValue {}

    record Bool(boolean value) implements TsValue {}

    record Str(String value) implements TsValue {}

    record Bytes(byte[] value) implements TsValue {
        @Override
        public boolean equals(Object o) {
            return o instanceof Bytes b && Arrays.equals(value, b.value);
        }

        @Override
        public int hashCode() {
            return Arrays.hashCode(value);
        }
    }

    record Null() implements TsValue {}

    record MapVal(Map<String, TsValue> value) implements TsValue {
        public MapVal {
            value = new TreeMap<>(value);
        }
    }

    record Array(List<TsValue> value) implements TsValue {
        public Array {
            value = List.copyOf(value);
        }
    }

    static TsValue ofLong(long v) {
        return new I64(v);
    }

    static TsValue ofDouble(double v) {
        return new F64(v);
    }

    static TsValue ofBool(boolean v) {
        return new Bool(v);
    }

    static TsValue ofString(String v) {
        return new Str(v);
    }

    static TsValue nullValue() {
        return new Null();
    }
}
