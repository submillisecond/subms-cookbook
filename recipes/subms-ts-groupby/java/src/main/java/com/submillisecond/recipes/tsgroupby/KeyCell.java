package com.submillisecond.recipes.tsgroupby;

import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsValue;

/**
 * A typed, hashable, totally-ordered projection of a {@link TsValue} cell, used
 * as a group key element. A {@code DoubleKey} keys on the bit pattern of its f64
 * (with {@code -0.0} normalised to {@code 0.0}) so equal values land in one
 * group; non-finite never reaches here (a series rejects it on ingest). Bytes /
 * Map / Array / Null cells are not legal keys ({@link #from} returns null).
 *
 * <p>The cross-type order mirrors the Rust sibling's derived enum ordering:
 * {@code I64} &lt; {@code F64} &lt; {@code Bool} &lt; {@code Str}.
 */
sealed interface KeyCell extends Comparable<KeyCell>
        permits KeyCell.LongKey, KeyCell.DoubleKey, KeyCell.BoolKey, KeyCell.StrKey {

    int rank();

    /** Project a cell into a key element, or null if absent / non-hashable. */
    static KeyCell from(Optional<TsValue> c) {
        if (c.isEmpty()) {
            return null;
        }
        TsValue v = c.get();
        if (v instanceof TsValue.I64 x) {
            return new LongKey(x.value());
        }
        if (v instanceof TsValue.F64 x) {
            return new DoubleKey(normaliseZero(x.value()));
        }
        if (v instanceof TsValue.Bool x) {
            return new BoolKey(x.value());
        }
        if (v instanceof TsValue.Str x) {
            return new StrKey(x.value());
        }
        return null;
    }

    // Normalise -0.0 to +0.0 so the two zeros share a key (and a group).
    private static double normaliseZero(double v) {
        return v == 0.0 ? 0.0 : v;
    }

    /** Lexicographic compare over two key tuples. */
    static int compare(List<KeyCell> a, List<KeyCell> b) {
        int n = Math.min(a.size(), b.size());
        for (int i = 0; i < n; i++) {
            int cmp = a.get(i).compareTo(b.get(i));
            if (cmp != 0) {
                return cmp;
            }
        }
        return Integer.compare(a.size(), b.size());
    }

    record LongKey(long value) implements KeyCell {
        @Override
        public int rank() {
            return 0;
        }

        @Override
        public int compareTo(KeyCell o) {
            return o instanceof LongKey k ? Long.compare(value, k.value) : Integer.compare(rank(), o.rank());
        }
    }

    record DoubleKey(double value) implements KeyCell {
        @Override
        public int rank() {
            return 1;
        }

        @Override
        public int compareTo(KeyCell o) {
            return o instanceof DoubleKey k ? Double.compare(value, k.value) : Integer.compare(rank(), o.rank());
        }
    }

    record BoolKey(boolean value) implements KeyCell {
        @Override
        public int rank() {
            return 2;
        }

        @Override
        public int compareTo(KeyCell o) {
            return o instanceof BoolKey k ? Boolean.compare(value, k.value) : Integer.compare(rank(), o.rank());
        }
    }

    record StrKey(String value) implements KeyCell {
        @Override
        public int rank() {
            return 3;
        }

        @Override
        public int compareTo(KeyCell o) {
            return o instanceof StrKey k ? value.compareTo(k.value) : Integer.compare(rank(), o.rank());
        }
    }
}
