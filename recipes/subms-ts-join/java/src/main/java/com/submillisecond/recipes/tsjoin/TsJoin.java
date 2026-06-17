package com.submillisecond.recipes.tsjoin;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The full equi-join matrix between two heterogeneous {@link TsDataFrame}s on
 * key column(s) of ANY type. Three execution strategies, six join kinds, one
 * output type.
 *
 * <ul>
 * <li>{@link #hashJoin} builds a hash table on the right input and probes with
 * the left - the workhorse for unsorted keys.</li>
 * <li>{@link #sortMergeJoin} sorts both inputs on the key tuple and merges in a
 * single linear pass - wins when inputs arrive sorted (the common time-series
 * case) or when the join is many-to-many and the hash table would balloon.</li>
 * <li>{@link #crossJoin} is the cartesian product, no keys.</li>
 * </ul>
 *
 * <h2>Keys are typed cells, not f64</h2>
 * A real join keys on a {@code Str} symbol, an {@code I64} date, a {@code Bool}
 * flag, or any tuple mixing them - not just {@code f64}. A key cell carries its
 * type into the key token, so a {@code Str} "AAPL" only ever joins another
 * {@code Str} "AAPL", never a numeric column that happens to share bits.
 *
 * <h2>Nulls are validity bits, not sentinels</h2>
 * An outer / left / right join emits rows where one side has no match. The
 * unmatched side's columns are MISSING there, not zero. We use {@link TsArray}'s
 * Arrow-style validity model: a missing cell sets {@code valid[i] = false} and
 * leaves {@code values[i]} unspecified. The output is a {@link TsJoinResult} of
 * named {@link TsArray}s, every one the same length, and a caller reads a cell
 * with {@link TsArray#get} (empty on a missing cell) or coalesces it with
 * {@link TsArray#fillNull}. This is the exact primitive {@code subms-ts-expr}
 * produces, so the join output drops straight into the expression evaluator.
 *
 * <h2>Column collisions</h2>
 * Both inputs can carry a column of the same name. The output renames the
 * collision with {@code _left} / {@code _right} suffixes (the join keys
 * themselves are emitted once, unsuffixed, from the left).
 *
 * <h2>This is the equi-join. asof lives elsewhere.</h2>
 * This recipe is the EQUALITY join matrix. The time-series asof / as-of-prior
 * join is its own recipe, {@code subms-ts-asof-join}. A {@code subms-ts-categorical}
 * dictionary can pre-encode string keys to {@code u32} for cheaper probes; that
 * is a composition, not a dependency here.
 */
public final class TsJoin {

    private TsJoin() {}

    // ---------- frame flattening ----------

    /**
     * A frame flattened to named, typed dense {@link TsArray}s over its
     * union-of-timestamps row axis - exactly the shape {@code subms-ts-expr}
     * evaluates against. Each column is dense over the row axis: a row where the
     * column had no point is a null cell (validity unset). Exposed so a caller
     * can inspect the same flattening the join uses.
     */
    public static FrameColumns frameColumns(TsDataFrame frame) {
        List<String> names = frame.columnNames();
        int ncols = names.size();
        TsDataType[] types = new TsDataType[ncols];
        for (int i = 0; i < ncols; i++) {
            TsColumn c = frame.column(names.get(i)).orElseThrow();
            types[i] = c.dataType();
        }
        List<List<Optional<TsValue>>> cells = new ArrayList<>(ncols);
        for (int i = 0; i < ncols; i++) {
            cells.add(new ArrayList<>());
        }
        for (TsDataFrame.Row row : frame.aligned()) {
            List<Optional<TsValue>> vals = row.values();
            for (int i = 0; i < ncols; i++) {
                cells.get(i).add(vals.get(i));
            }
        }
        List<TsArray> columns = new ArrayList<>(ncols);
        for (int i = 0; i < ncols; i++) {
            columns.add(cellsToArray(types[i], cells.get(i)));
        }
        return new FrameColumns(names, columns);
    }

    /** The names + typed columns a frame flattens to, mirroring the Rust return. */
    public record FrameColumns(List<String> names, List<TsArray> columns) {}

    // Project a column's per-row optional cells onto a typed TsArray of the
    // column's declared dtype. A cell whose boxed type disagrees is treated as
    // null (a stored column never mixes types, so this is defensive).
    private static TsArray cellsToArray(TsDataType ty, List<Optional<TsValue>> cells) {
        int n = cells.size();
        switch (ty) {
            case I64 -> {
                long[] values = new long[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.I64 v) {
                        values[i] = v.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.I64(values, valid);
            }
            case BOOL -> {
                boolean[] values = new boolean[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.Bool v) {
                        values[i] = v.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.Bool(values, valid);
            }
            case STR -> {
                String[] values = new String[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.Str v) {
                        values[i] = v.value();
                        valid[i] = true;
                    } else {
                        values[i] = "";
                    }
                }
                return new TsArray.Str(values, valid);
            }
            default -> {
                // F64 and the schemaless VALUE escape hatch both land in an f64
                // array, matching the expr evaluator's flattening.
                double[] values = new double[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Double d = cells.get(i).flatMap(TsJoin::valueAsF64).orElse(null);
                    if (d != null) {
                        values[i] = d;
                        valid[i] = true;
                    }
                }
                return new TsArray.F64(values, valid);
            }
        }
    }

    private static Optional<Double> valueAsF64(TsValue v) {
        if (v instanceof TsValue.F64 f) {
            return Optional.of(f.value());
        }
        if (v instanceof TsValue.I64 i) {
            return Optional.of((double) i.value());
        }
        return Optional.empty();
    }

    // A flattened input ready to join: column names + typed columns + resolved
    // key indices + the row count.
    private static final class Table {
        final List<String> names;
        final List<TsArray> columns;
        final int[] keyIdx;
        final int nrows;

        private Table(List<String> names, List<TsArray> columns, int[] keyIdx, int nrows) {
            this.names = names;
            this.columns = columns;
            this.keyIdx = keyIdx;
            this.nrows = nrows;
        }

        static Table build(TsDataFrame frame, String[] keys, String side) {
            FrameColumns fc = frameColumns(frame);
            List<String> names = fc.names();
            List<TsArray> columns = fc.columns();
            int nrows = columns.isEmpty() ? 0 : columns.get(0).len();
            int[] keyIdx = new int[keys.length];
            for (int k = 0; k < keys.length; k++) {
                int idx = names.indexOf(keys[k]);
                if (idx < 0) {
                    throw TsJoinException.unknownKey(side, keys[k]);
                }
                keyIdx[k] = idx;
            }
            return new Table(names, columns, keyIdx, nrows);
        }

        // The key tuple at a row. A null key cell makes the whole tuple
        // unmatchable - in SQL semantics a NULL key never equals anything.
        KeyTuple keyAt(int row) {
            KeyToken[] parts = new KeyToken[keyIdx.length];
            boolean matchable = true;
            for (int i = 0; i < keyIdx.length; i++) {
                KeyToken tok = keyToken(columns.get(keyIdx[i]), row);
                if (tok == null) {
                    matchable = false;
                    parts[i] = KeyToken.NULL;
                } else {
                    parts[i] = tok;
                }
            }
            return new KeyTuple(parts, matchable);
        }

        boolean keyContains(int ci) {
            for (int k : keyIdx) {
                if (k == ci) {
                    return true;
                }
            }
            return false;
        }
    }

    // A single typed key component, hashable and totally ordered. The f64 bit
    // pattern is the F64 equality token: -0.0 and +0.0 differ in bits and so do
    // NOT join (a documented non-claim, matching SQL/Polars float-key behaviour).
    private static KeyToken keyToken(TsArray col, int row) {
        Optional<TsValue> cell = col.get(row);
        if (cell.isEmpty()) {
            return null;
        }
        TsValue v = cell.get();
        if (v instanceof TsValue.Bool b) {
            return new KeyToken(KeyToken.Tag.BOOL, b.value() ? 1L : 0L, null);
        }
        if (v instanceof TsValue.I64 i) {
            return new KeyToken(KeyToken.Tag.I64, i.value(), null);
        }
        if (v instanceof TsValue.F64 f) {
            return new KeyToken(KeyToken.Tag.F64, Double.doubleToRawLongBits(f.value()), null);
        }
        if (v instanceof TsValue.Str s) {
            return new KeyToken(KeyToken.Tag.STR, 0L, s.value());
        }
        return null;
    }

    private static final class KeyToken implements Comparable<KeyToken> {
        enum Tag {
            NULL,
            BOOL,
            I64,
            F64,
            STR
        }

        static final KeyToken NULL = new KeyToken(Tag.NULL, 0L, null);

        final Tag tag;
        final long bits; // BOOL / I64 / F64-bit-pattern payload
        final String str; // STR payload

        KeyToken(Tag tag, long bits, String str) {
            this.tag = tag;
            this.bits = bits;
            this.str = str;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) {
                return true;
            }
            if (!(o instanceof KeyToken k) || tag != k.tag) {
                return false;
            }
            if (tag == Tag.STR) {
                return java.util.Objects.equals(str, k.str);
            }
            return bits == k.bits;
        }

        @Override
        public int hashCode() {
            if (tag == Tag.STR) {
                return 31 * tag.ordinal() + java.util.Objects.hashCode(str);
            }
            return 31 * tag.ordinal() + Long.hashCode(bits);
        }

        @Override
        public int compareTo(KeyToken o) {
            if (tag != o.tag) {
                return Integer.compare(tag.ordinal(), o.tag.ordinal());
            }
            if (tag == Tag.STR) {
                return str.compareTo(o.str);
            }
            if (tag == Tag.F64) {
                // unsigned to match the Rust u64-bits lexicographic order.
                return Long.compareUnsigned(bits, o.bits);
            }
            return Long.compare(bits, o.bits);
        }
    }

    private static final class KeyTuple {
        final KeyToken[] parts;
        final boolean matchable;

        KeyTuple(KeyToken[] parts, boolean matchable) {
            this.parts = parts;
            this.matchable = matchable;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) {
                return true;
            }
            if (!(o instanceof KeyTuple k)) {
                return false;
            }
            return matchable == k.matchable && java.util.Arrays.equals(parts, k.parts);
        }

        @Override
        public int hashCode() {
            return 31 * java.util.Arrays.hashCode(parts) + (matchable ? 1 : 0);
        }
    }

    // ---------- output assembly ----------

    // A row of the output is "left row L (or -1) paired with right row R (or
    // -1)". -1 on either side means that side is missing.
    private record Pair(int left, int right) {}

    private static final int NONE = -1;

    private static TsJoinResult assemble(Table left, Table right, List<Pair> pairs, TsJoinKind kind) {
        int nrows = pairs.size();
        List<String> names = new ArrayList<>();
        List<TsArray> columns = new ArrayList<>();

        // Key columns once, from whichever side is present per row (left wins
        // when both present; an outer right-only row reads the key off right).
        for (int li = 0; li < left.keyIdx.length; li++) {
            int lk = left.keyIdx[li];
            int rk = right.keyIdx[li];
            TsArray lcol = left.columns.get(lk);
            TsArray rcol = right.columns.get(rk);
            ArrayBuilder b = ArrayBuilder.forType(lcol.dataType(), nrows);
            for (Pair p : pairs) {
                Optional<TsValue> cell = cellAt(lcol, p.left());
                if (cell.isEmpty()) {
                    cell = cellAt(rcol, p.right());
                }
                b.push(cell);
            }
            names.add(left.names.get(lk));
            columns.add(b.finish());
        }

        // Semi / Anti emit left non-key columns only.
        boolean emitRight = !(kind == TsJoinKind.SEMI || kind == TsJoinKind.ANTI);

        for (int ci = 0; ci < left.names.size(); ci++) {
            if (left.keyContains(ci)) {
                continue;
            }
            String name = left.names.get(ci);
            String outName = (emitRight && rightHasPayload(right, name)) ? name + "_left" : name;
            names.add(outName);
            columns.add(project(left.columns.get(ci), pairs, Side.LEFT));
        }

        if (emitRight) {
            for (int ci = 0; ci < right.names.size(); ci++) {
                if (right.keyContains(ci)) {
                    continue;
                }
                String name = right.names.get(ci);
                String outName = leftHasPayload(left, name) ? name + "_right" : name;
                names.add(outName);
                columns.add(project(right.columns.get(ci), pairs, Side.RIGHT));
            }
        }

        return new TsJoinResult(names, columns, nrows);
    }

    private static boolean rightHasPayload(Table right, String name) {
        for (int rj = 0; rj < right.names.size(); rj++) {
            if (right.names.get(rj).equals(name) && !right.keyContains(rj)) {
                return true;
            }
        }
        return false;
    }

    private static boolean leftHasPayload(Table left, String name) {
        for (int lj = 0; lj < left.names.size(); lj++) {
            if (left.names.get(lj).equals(name) && !left.keyContains(lj)) {
                return true;
            }
        }
        return false;
    }

    private enum Side {
        LEFT,
        RIGHT
    }

    private static TsArray project(TsArray src, List<Pair> pairs, Side side) {
        ArrayBuilder b = ArrayBuilder.forType(src.dataType(), pairs.size());
        for (Pair p : pairs) {
            int srcRow = side == Side.LEFT ? p.left() : p.right();
            b.push(cellAt(src, srcRow));
        }
        return b.finish();
    }

    private static Optional<TsValue> cellAt(TsArray col, int row) {
        if (row == NONE) {
            return Optional.empty();
        }
        return col.get(row);
    }

    // Accumulates typed cells into the matching TsArray variant. A cell whose
    // boxed type disagrees with the builder's type is recorded as null.
    private static final class ArrayBuilder {
        private final TsDataType ty;
        private final double[] dv;
        private final long[] lv;
        private final boolean[] bv;
        private final String[] sv;
        private final boolean[] valid;
        private int n;

        private ArrayBuilder(TsDataType ty, int cap) {
            this.ty = ty;
            this.valid = new boolean[cap];
            switch (ty) {
                case I64 -> {
                    lv = new long[cap];
                    dv = null;
                    bv = null;
                    sv = null;
                }
                case BOOL -> {
                    bv = new boolean[cap];
                    dv = null;
                    lv = null;
                    sv = null;
                }
                case STR -> {
                    sv = new String[cap];
                    dv = null;
                    lv = null;
                    bv = null;
                }
                default -> {
                    dv = new double[cap];
                    lv = null;
                    bv = null;
                    sv = null;
                }
            }
        }

        static ArrayBuilder forType(TsDataType ty, int cap) {
            return new ArrayBuilder(ty, cap);
        }

        void push(Optional<TsValue> cell) {
            switch (ty) {
                case I64 -> {
                    if (cell.isPresent() && cell.get() instanceof TsValue.I64 v) {
                        lv[n] = v.value();
                        valid[n] = true;
                    }
                }
                case BOOL -> {
                    if (cell.isPresent() && cell.get() instanceof TsValue.Bool v) {
                        bv[n] = v.value();
                        valid[n] = true;
                    }
                }
                case STR -> {
                    if (cell.isPresent() && cell.get() instanceof TsValue.Str v) {
                        sv[n] = v.value();
                        valid[n] = true;
                    } else {
                        sv[n] = "";
                    }
                }
                default -> {
                    Double d = cell.flatMap(TsJoin::valueAsF64).orElse(null);
                    if (d != null) {
                        dv[n] = d;
                        valid[n] = true;
                    }
                }
            }
            n++;
        }

        TsArray finish() {
            return switch (ty) {
                case I64 -> new TsArray.I64(lv, valid);
                case BOOL -> new TsArray.Bool(bv, valid);
                case STR -> new TsArray.Str(sv, valid);
                default -> new TsArray.F64(dv, valid);
            };
        }
    }

    // ---------- public join entry points ----------

    private static void validateKeys(String[] leftKeys, String[] rightKeys) {
        if (leftKeys.length == 0) {
            throw TsJoinException.noKeys();
        }
        if (leftKeys.length != rightKeys.length) {
            throw TsJoinException.keyArityMismatch(leftKeys.length, rightKeys.length);
        }
    }

    /**
     * Equi-join via a hash table on the right side, probed by the left.
     *
     * <p>Keys are typed cells: a {@code Str} symbol joins a {@code Str} symbol,
     * an {@code I64} date an {@code I64} date, a {@code (Str, I64)} tuple another.
     *
     * <p>Row order is deterministic: the output walks the LEFT input top to
     * bottom (driving order), and for each left row emits its right matches in
     * the right input's original row order. Outer-only right rows come last, in
     * right-input order.
     */
    public static TsJoinResult hashJoin(
            TsDataFrame left,
            TsDataFrame right,
            String[] leftKeys,
            String[] rightKeys,
            TsJoinKind kind) {
        validateKeys(leftKeys, rightKeys);
        Table l = Table.build(left, leftKeys, "left");
        Table r = Table.build(right, rightKeys, "right");

        // Build a key -> right-row-list index once. Insertion-ordered list per
        // key preserves right-input order for deterministic output.
        Map<KeyTuple, List<Integer>> index = new HashMap<>();
        for (int rr = 0; rr < r.nrows; rr++) {
            KeyTuple key = r.keyAt(rr);
            if (!key.matchable) {
                continue;
            }
            index.computeIfAbsent(key, k -> new ArrayList<>()).add(rr);
        }

        List<Pair> pairs = new ArrayList<>();
        boolean[] rightMatched = new boolean[r.nrows];

        for (int li = 0; li < l.nrows; li++) {
            KeyTuple key = l.keyAt(li);
            List<Integer> matches = key.matchable ? index.get(key) : null;
            switch (kind) {
                case SEMI -> {
                    if (matches != null) {
                        pairs.add(new Pair(li, NONE));
                    }
                }
                case ANTI -> {
                    if (matches == null) {
                        pairs.add(new Pair(li, NONE));
                    }
                }
                default -> {
                    if (matches != null) {
                        for (int rr : matches) {
                            rightMatched[rr] = true;
                            pairs.add(new Pair(li, rr));
                        }
                    } else if (kind == TsJoinKind.LEFT || kind == TsJoinKind.OUTER) {
                        pairs.add(new Pair(li, NONE));
                    }
                }
            }
        }

        if (kind == TsJoinKind.RIGHT || kind == TsJoinKind.OUTER) {
            for (int rr = 0; rr < r.nrows; rr++) {
                if (!rightMatched[rr]) {
                    pairs.add(new Pair(NONE, rr));
                }
            }
        }

        return assemble(l, r, pairs, kind);
    }

    /**
     * Equi-join via sort-merge: both inputs are sorted on the key tuple, then
     * merged in a single linear pass. Produces the same result SET as
     * {@link #hashJoin} for every kind. Row order differs: output is in
     * sorted-key order (then, within a key group, left-input order x
     * right-input order), the natural order for already-sorted inputs.
     *
     * <p>Unmatchable (null-key) rows are handled like SQL NULL keys: they never
     * merge, and surface as unmatched-left / unmatched-right per the kind.
     */
    public static TsJoinResult sortMergeJoin(
            TsDataFrame left,
            TsDataFrame right,
            String[] leftKeys,
            String[] rightKeys,
            TsJoinKind kind) {
        validateKeys(leftKeys, rightKeys);
        Table l = Table.build(left, leftKeys, "left");
        Table r = Table.build(right, rightKeys, "right");

        // Precompute key tuples once; sort row indices by key. Unmatchable rows
        // sort to the end and are never merged.
        KeyTuple[] lkeys = new KeyTuple[l.nrows];
        KeyTuple[] rkeys = new KeyTuple[r.nrows];
        for (int i = 0; i < l.nrows; i++) {
            lkeys[i] = l.keyAt(i);
        }
        for (int i = 0; i < r.nrows; i++) {
            rkeys[i] = r.keyAt(i);
        }
        Integer[] lorderBox = boxedRange(l.nrows);
        Integer[] rorderBox = boxedRange(r.nrows);
        java.util.Arrays.sort(lorderBox, (a, b) -> cmpKey(lkeys[a], lkeys[b]));
        java.util.Arrays.sort(rorderBox, (a, b) -> cmpKey(rkeys[a], rkeys[b]));
        int[] lorder = unbox(lorderBox);
        int[] rorder = unbox(rorderBox);

        List<Pair> pairs = new ArrayList<>();
        int li = 0;
        int ri = 0;
        boolean emitUnmatchedLeft =
                kind == TsJoinKind.LEFT || kind == TsJoinKind.OUTER || kind == TsJoinKind.ANTI;
        boolean emitUnmatchedRight = kind == TsJoinKind.RIGHT || kind == TsJoinKind.OUTER;

        while (li < l.nrows) {
            KeyTuple lkey = lkeys[lorder[li]];
            if (!lkey.matchable) {
                break;
            }
            // advance right past keys strictly less than the current left key.
            while (ri < r.nrows) {
                KeyTuple rkey = rkeys[rorder[ri]];
                if (!rkey.matchable || cmpKey(rkey, lkey) >= 0) {
                    break;
                }
                if (emitUnmatchedRight) {
                    pairs.add(new Pair(NONE, rorder[ri]));
                }
                ri++;
            }
            // gather the left group sharing lkey.
            int lstart = li;
            while (li < l.nrows) {
                KeyTuple k = lkeys[lorder[li]];
                if (k.matchable && cmpKey(k, lkey) == 0) {
                    li++;
                } else {
                    break;
                }
            }
            // gather the right group sharing lkey.
            int rstart = ri;
            while (ri < r.nrows) {
                KeyTuple k = rkeys[rorder[ri]];
                if (k.matchable && cmpKey(k, lkey) == 0) {
                    ri++;
                } else {
                    break;
                }
            }
            emitGroup(pairs, lorder, lstart, li, rorder, rstart, ri, kind, emitUnmatchedLeft);
        }

        // any left rows left over with no right match (right exhausted).
        while (li < l.nrows) {
            if (emitUnmatchedLeft && kind != TsJoinKind.SEMI) {
                pairs.add(new Pair(lorder[li], NONE));
            }
            li++;
        }
        // any trailing right rows (their key exceeded every left key).
        while (ri < r.nrows) {
            KeyTuple rkey = rkeys[rorder[ri]];
            if (rkey.matchable && emitUnmatchedRight) {
                pairs.add(new Pair(NONE, rorder[ri]));
            }
            ri++;
        }

        return assemble(l, r, pairs, kind);
    }

    private static void emitGroup(
            List<Pair> pairs,
            int[] lorder,
            int lstart,
            int lend,
            int[] rorder,
            int rstart,
            int rend,
            TsJoinKind kind,
            boolean emitUnmatchedLeft) {
        boolean hasRight = rend > rstart;
        switch (kind) {
            case SEMI -> {
                if (hasRight) {
                    for (int i = lstart; i < lend; i++) {
                        pairs.add(new Pair(lorder[i], NONE));
                    }
                }
            }
            case ANTI -> {
                if (!hasRight) {
                    for (int i = lstart; i < lend; i++) {
                        pairs.add(new Pair(lorder[i], NONE));
                    }
                }
            }
            default -> {
                if (hasRight) {
                    for (int i = lstart; i < lend; i++) {
                        for (int j = rstart; j < rend; j++) {
                            pairs.add(new Pair(lorder[i], rorder[j]));
                        }
                    }
                } else if (emitUnmatchedLeft) {
                    for (int i = lstart; i < lend; i++) {
                        pairs.add(new Pair(lorder[i], NONE));
                    }
                }
            }
        }
    }

    // Unmatchable (null-key) tuples sort after everything so the merge can stop
    // the moment it hits the first one.
    private static int cmpKey(KeyTuple a, KeyTuple b) {
        if (a.matchable && b.matchable) {
            int n = Math.min(a.parts.length, b.parts.length);
            for (int i = 0; i < n; i++) {
                int c = a.parts[i].compareTo(b.parts[i]);
                if (c != 0) {
                    return c;
                }
            }
            return Integer.compare(a.parts.length, b.parts.length);
        }
        if (a.matchable) {
            return -1;
        }
        if (b.matchable) {
            return 1;
        }
        return 0;
    }

    private static Integer[] boxedRange(int n) {
        Integer[] out = new Integer[n];
        for (int i = 0; i < n; i++) {
            out[i] = i;
        }
        return out;
    }

    private static int[] unbox(Integer[] in) {
        int[] out = new int[in.length];
        for (int i = 0; i < in.length; i++) {
            out[i] = in[i];
        }
        return out;
    }

    /**
     * The keyless cartesian product: every left row paired with every right
     * row. Output row count is {@code left.nrows * right.nrows}; order is
     * left-major (left row 0 against every right row, then left row 1, ...).
     * All cells are present - a cross join never produces a null. Column
     * collisions are suffixed {@code _left} / {@code _right}.
     */
    public static TsJoinResult crossJoin(TsDataFrame left, TsDataFrame right) {
        Table l = Table.build(left, new String[0], "left");
        Table r = Table.build(right, new String[0], "right");
        List<Pair> pairs = new ArrayList<>(l.nrows * r.nrows);
        for (int li = 0; li < l.nrows; li++) {
            for (int ri = 0; ri < r.nrows; ri++) {
                pairs.add(new Pair(li, ri));
            }
        }
        return assemble(l, r, pairs, TsJoinKind.INNER);
    }
}
