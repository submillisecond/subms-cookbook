package com.submillisecond.recipes.tsreshape;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The reshape half of the Polars / DuckDB relational surface over a
 * heterogeneous {@link TsDataFrame}: the long-to-wide {@link #pivot}, the
 * wide-to-long {@link #melt} (unpivot), the list-cell {@link #explode}, the
 * {@link #vstack} / {@link #hstack} concatenators, and the row set-ops
 * {@link #union} / {@link #intersect} / {@link #except}.
 *
 * <p>Every operation flattens its input(s) to dense named, typed {@link TsArray}s
 * over the union-of-timestamps row axis (the same flattening
 * {@code subms-ts-expr}, {@code subms-ts-join}, and {@code subms-ts-groupby}
 * use), reshapes, and returns a {@link TsReshapeResult} of named {@link TsArray}s.
 *
 * <h2>The frame is per-column typed, so reshaping speaks types</h2>
 * <ul>
 * <li>{@link #pivot} takes the DISTINCT values of {@code columnsCol} - typically
 * a {@code Str} symbol - and turns each into an output column named by the
 * value. Cells are numeric aggregates (Sum / Mean / Min / Max / Last).</li>
 * <li>{@link #melt} is wide-to-long: it emits a real {@code Str}
 * {@code "variable"} column naming the source slot per row, plus a
 * {@code value} column. The string variable column is the capability the old
 * f64-only frame could not express.</li>
 * <li>{@link #explode} walks a {@code Value} column of {@link TsValue.Array}
 * cells and emits one row per element; an empty list drops the row.</li>
 * </ul>
 *
 * <h2>Absent cells are validity bits, not sentinels</h2>
 * A pivot's (index, column) pair with no source rows, or a melt over a column
 * missing at a row, is a null cell ({@code valid[i] = false}), not a zero.
 *
 * <h2>Row equality is the typed cell tuple</h2>
 * The set-ops compare each row as a tuple of typed cells: {@code F64} by bit
 * pattern, {@code Str} by value, a missing cell as its own token. A {@code Str}
 * "3" never equals a numeric {@code 3}; {@code -0.0} and {@code +0.0} are
 * distinct rows.
 *
 * <h2>Contract</h2>
 * Throughput-contracted, NOT per-op sub-ms. The honest number is rows/sec,
 * captured in {@code perf/{rust,java}.json}.
 */
public final class TsReshape {

    private TsReshape() {}

    // ---------- frame flattening ----------

    /**
     * Flatten a frame to named, typed {@link TsArray}s over its
     * union-of-timestamps row axis. Each column is dense over the row axis: a row
     * where the column had no point is a null cell (validity unset). A
     * {@code Value} column lands in an f64 array here; its raw boxed cells are
     * available via {@link #frameValueCells} for {@link #explode}.
     */
    public static FrameColumns frameColumns(TsDataFrame frame) {
        return flatten(frame).columns;
    }

    /**
     * The per-row boxed cells of every {@code Value}-typed column, keyed by
     * column name. Used by {@link #explode}, whose list cells are
     * {@link TsValue.Array} documents an f64 {@link TsArray} cannot carry.
     */
    public static Map<String, List<Optional<TsValue>>> frameValueCells(TsDataFrame frame) {
        return flatten(frame).valueCells;
    }

    /** The named + typed columns a frame flattens to. */
    public record FrameColumns(List<String> names, List<TsArray> columns) {}

    private record Flattened(FrameColumns columns, Map<String, List<Optional<TsValue>>> valueCells) {}

    private static Flattened flatten(TsDataFrame frame) {
        List<String> names = frame.columnNames();
        int ncols = names.size();
        TsDataType[] types = new TsDataType[ncols];
        for (int i = 0; i < ncols; i++) {
            types[i] = frame.column(names.get(i)).orElseThrow().dataType();
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
        Map<String, List<Optional<TsValue>>> valueCells = new HashMap<>();
        for (int i = 0; i < ncols; i++) {
            if (types[i] == TsDataType.VALUE) {
                valueCells.put(names.get(i), cells.get(i));
            }
            columns.add(cellsToArray(types[i], cells.get(i)));
        }
        return new Flattened(new FrameColumns(names, columns), valueCells);
    }

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
                double[] values = new double[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Double d = cells.get(i).flatMap(TsReshape::valueAsDouble).orElse(null);
                    if (d != null) {
                        values[i] = d;
                        valid[i] = true;
                    }
                }
                return new TsArray.F64(values, valid);
            }
        }
    }

    private static Optional<Double> valueAsDouble(TsValue v) {
        if (v instanceof TsValue.F64 f) {
            return Optional.of(f.value());
        }
        if (v instanceof TsValue.I64 i) {
            return Optional.of((double) i.value());
        }
        return Optional.empty();
    }

    private static int nrowsOf(List<TsArray> cols) {
        return cols.isEmpty() ? 0 : cols.get(0).len();
    }

    private static int resolve(List<String> names, String col) {
        int i = names.indexOf(col);
        if (i < 0) {
            throw TsReshapeException.unknownColumn(col);
        }
        return i;
    }

    // The shared dtype of a set of columns, or empty when they disagree.
    private static Optional<TsDataType> commonType(List<TsDataType> types) {
        if (types.isEmpty()) {
            return Optional.empty();
        }
        TsDataType first = types.get(0);
        for (TsDataType t : types) {
            if (t != first) {
                return Optional.empty();
            }
        }
        return Optional.of(first);
    }

    // ---------- typed array builder ----------

    // Accumulates typed cells into the matching TsArray variant. A cell whose
    // boxed type disagrees with the builder's type is recorded as null. The same
    // builder the join uses, so reshape outputs share a wire shape.
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
                    Double d = cell.flatMap(TsReshape::valueAsDouble).orElse(null);
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

    // ---------- pivot ----------

    // A running bucket accumulator: everything an agg might need so one state
    // serves Sum / Mean / Min / Max / Last without re-scanning.
    private static final class Acc {
        double sum;
        long count;
        double min;
        double max;
        double last;

        Acc(double v) {
            sum = v;
            count = 1;
            min = v;
            max = v;
            last = v;
        }

        void fold(double v) {
            sum += v;
            count++;
            if (v < min) {
                min = v;
            }
            if (v > max) {
                max = v;
            }
            last = v;
        }

        double finish(PivotAgg agg) {
            return switch (agg) {
                case SUM -> sum;
                case MEAN -> sum / count;
                case MIN -> min;
                case MAX -> max;
                case LAST -> last;
            };
        }
    }

    // A type-tagged identity token for an index / category cell, ordered so the
    // category axis is laid out deterministically. A Str "3" never collides with
    // a numeric 3.
    private static final class CellKey implements Comparable<CellKey> {
        enum Tag {
            BOOL,
            I64,
            F64,
            STR
        }

        final Tag tag;
        final long bits;
        final String str;

        private CellKey(Tag tag, long bits, String str) {
            this.tag = tag;
            this.bits = bits;
            this.str = str;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) {
                return true;
            }
            if (!(o instanceof CellKey k) || tag != k.tag) {
                return false;
            }
            return tag == Tag.STR ? Objects.equals(str, k.str) : bits == k.bits;
        }

        @Override
        public int hashCode() {
            return tag == Tag.STR
                    ? 31 * tag.ordinal() + Objects.hashCode(str)
                    : 31 * tag.ordinal() + Long.hashCode(bits);
        }

        @Override
        public int compareTo(CellKey o) {
            if (tag != o.tag) {
                return Integer.compare(tag.ordinal(), o.tag.ordinal());
            }
            if (tag == Tag.STR) {
                return str.compareTo(o.str);
            }
            if (tag == Tag.F64) {
                return Long.compareUnsigned(bits, o.bits);
            }
            return Long.compare(bits, o.bits);
        }
    }

    private static CellKey cellKey(TsArray arr, int row) {
        Optional<TsValue> cell = arr.get(row);
        if (cell.isEmpty()) {
            return null;
        }
        TsValue v = cell.get();
        if (v instanceof TsValue.Bool b) {
            return new CellKey(CellKey.Tag.BOOL, b.value() ? 1L : 0L, null);
        }
        if (v instanceof TsValue.I64 i) {
            return new CellKey(CellKey.Tag.I64, i.value(), null);
        }
        if (v instanceof TsValue.F64 f) {
            return new CellKey(CellKey.Tag.F64, Double.doubleToRawLongBits(f.value()), null);
        }
        if (v instanceof TsValue.Str s) {
            return new CellKey(CellKey.Tag.STR, 0L, s.value());
        }
        return null;
    }

    // The output column NAME for a category value. A Str names by its own text;
    // a numeric one stringifies (integral without a decimal point). Mirrors the
    // Rust side so pivot column names are byte-equivalent across the ports.
    private static String categoryName(TsValue v) {
        if (v instanceof TsValue.Str s) {
            return s.value();
        }
        if (v instanceof TsValue.Bool b) {
            return Boolean.toString(b.value());
        }
        if (v instanceof TsValue.I64 i) {
            return Long.toString(i.value());
        }
        if (v instanceof TsValue.F64 f) {
            return formatDouble(f.value());
        }
        return "";
    }

    private static String formatDouble(double v) {
        if (Double.isFinite(v) && v == Math.rint(v) && Math.abs(v) < 1e15) {
            return Long.toString((long) v);
        }
        return Double.toString(v);
    }

    /**
     * Long-to-wide pivot. Rows are keyed by the distinct values of
     * {@code indexCol}; one output column is produced per distinct value of
     * {@code columnsCol}, named by that value (a {@code Str} category names by its
     * own text; a numeric one stringifies). Each cell is the {@code agg} of
     * {@code valuesCol} over the rows matching that (index, column) pair.
     *
     * <p>The output carries the index column first (typed), then one column per
     * distinct category in ascending category order. Distinct index values are
     * emitted in first-seen (input) order. An (index, column) pair with no source
     * rows is an absent cell. Rows where the index, column, or value cell is
     * missing are skipped. The value aggregates are {@code F64} columns; the index
     * column keeps its own type.
     */
    public static TsReshapeResult pivot(
            TsDataFrame frame,
            String indexCol,
            String columnsCol,
            String valuesCol,
            PivotAgg agg) {
        FrameColumns fc = frameColumns(frame);
        List<String> names = fc.names();
        List<TsArray> columns = fc.columns();
        int idxI = resolve(names, indexCol);
        int colI = resolve(names, columnsCol);
        int valI = resolve(names, valuesCol);
        int nrows = nrowsOf(columns);

        TsArray idx = columns.get(idxI);
        TsArray cat = columns.get(colI);
        TsArray val = columns.get(valI);

        List<TsValue> indexOrder = new ArrayList<>();
        Map<CellKey, Integer> indexRow = new HashMap<>();
        Map<CellKey, TsValue> categorySet = new HashMap<>();
        Map<BucketKey, Acc> buckets = new HashMap<>();

        for (int r = 0; r < nrows; r++) {
            CellKey ikey = cellKey(idx, r);
            CellKey ckey = cellKey(cat, r);
            Double vv = val.get(r).flatMap(TsReshape::valueAsDouble).orElse(null);
            if (ikey == null || ckey == null || vv == null) {
                continue;
            }
            Integer outRow = indexRow.get(ikey);
            if (outRow == null) {
                outRow = indexOrder.size();
                indexOrder.add(idx.get(r).orElseThrow());
                indexRow.put(ikey, outRow);
            }
            categorySet.putIfAbsent(ckey, cat.get(r).orElseThrow());
            BucketKey bk = new BucketKey(outRow, ckey);
            Acc acc = buckets.get(bk);
            if (acc == null) {
                buckets.put(bk, new Acc(vv));
            } else {
                acc.fold(vv);
            }
        }

        List<Map.Entry<CellKey, TsValue>> categories = new ArrayList<>(categorySet.entrySet());
        categories.sort((a, b) -> a.getKey().compareTo(b.getKey()));

        int outRows = indexOrder.size();
        List<String> outNames = new ArrayList<>(1 + categories.size());
        List<TsArray> outCols = new ArrayList<>(1 + categories.size());

        ArrayBuilder idxBuilder = ArrayBuilder.forType(idx.dataType(), outRows);
        for (TsValue v : indexOrder) {
            idxBuilder.push(Optional.of(v));
        }
        outNames.add(indexCol);
        outCols.add(idxBuilder.finish());

        for (Map.Entry<CellKey, TsValue> e : categories) {
            ArrayBuilder builder = ArrayBuilder.forType(TsDataType.F64, outRows);
            for (int outRow = 0; outRow < outRows; outRow++) {
                Acc acc = buckets.get(new BucketKey(outRow, e.getKey()));
                builder.push(acc == null
                        ? Optional.empty()
                        : Optional.of(TsValue.ofDouble(acc.finish(agg))));
            }
            outNames.add(categoryName(e.getValue()));
            outCols.add(builder.finish());
        }

        return new TsReshapeResult(outNames, outCols, outRows);
    }

    private record BucketKey(int outRow, CellKey category) {}

    // ---------- melt / unpivot ----------

    /**
     * Wide-to-long unpivot. For each input row x each {@code valueCol}, emit one
     * output row of {@code { idCols..., variable, value }}. {@code variable} is a
     * real {@code Str} column carrying the NAME of the value column the cell came
     * from; {@code value} carries the cell. The id columns repeat across the value
     * columns for a given input row.
     *
     * <p>{@code value}'s type is the shared dtype of every {@code valueCol} when
     * they agree; when they mix, {@code value} is {@code Str} (each cell
     * stringified). A missing source cell becomes a null in {@code value}. Output
     * row order is input-row-major, then value-column order within a row.
     *
     * <p>This is the headline capability the typed frame unlocks: {@code variable}
     * is a genuine string column the old f64-only frame could not hold.
     */
    public static TsReshapeResult melt(TsDataFrame frame, String[] idCols, String[] valueCols) {
        if (valueCols.length == 0) {
            throw TsReshapeException.noColumns();
        }
        FrameColumns fc = frameColumns(frame);
        List<String> names = fc.names();
        List<TsArray> columns = fc.columns();
        int nrows = nrowsOf(columns);

        int[] idIdx = new int[idCols.length];
        for (int i = 0; i < idCols.length; i++) {
            idIdx[i] = resolve(names, idCols[i]);
        }
        int[] valIdx = new int[valueCols.length];
        List<TsDataType> valTypes = new ArrayList<>(valueCols.length);
        for (int i = 0; i < valueCols.length; i++) {
            valIdx[i] = resolve(names, valueCols[i]);
            valTypes.add(columns.get(valIdx[i]).dataType());
        }
        TsDataType valueType = commonType(valTypes).orElse(TsDataType.STR);

        int outRows = nrows * valueCols.length;
        List<String> outNames = new ArrayList<>(idCols.length + 2);
        List<TsArray> outCols = new ArrayList<>(idCols.length + 2);

        for (int ci : idIdx) {
            TsArray src = columns.get(ci);
            ArrayBuilder builder = ArrayBuilder.forType(src.dataType(), outRows);
            for (int r = 0; r < nrows; r++) {
                Optional<TsValue> cell = src.get(r);
                for (int j = 0; j < valueCols.length; j++) {
                    builder.push(cell);
                }
            }
            outNames.add(names.get(ci));
            outCols.add(builder.finish());
        }

        String[] varValues = new String[outRows];
        boolean[] varValid = new boolean[outRows];
        int vi = 0;
        for (int r = 0; r < nrows; r++) {
            for (String vc : valueCols) {
                varValues[vi] = vc;
                varValid[vi] = true;
                vi++;
            }
        }
        outNames.add("variable");
        outCols.add(new TsArray.Str(varValues, varValid));

        ArrayBuilder valBuilder = ArrayBuilder.forType(valueType, outRows);
        for (int r = 0; r < nrows; r++) {
            for (int idx : valIdx) {
                Optional<TsValue> cell = columns.get(idx).get(r);
                Optional<TsValue> coerced = valueType == TsDataType.STR
                        ? cell.map(c -> TsValue.ofString(stringifyCell(c)))
                        : cell;
                valBuilder.push(coerced);
            }
        }
        outNames.add("value");
        outCols.add(valBuilder.finish());

        return new TsReshapeResult(outNames, outCols, outRows);
    }

    private static String stringifyCell(TsValue v) {
        if (v instanceof TsValue.Str s) {
            return s.value();
        }
        if (v instanceof TsValue.Bool b) {
            return Boolean.toString(b.value());
        }
        if (v instanceof TsValue.I64 i) {
            return Long.toString(i.value());
        }
        if (v instanceof TsValue.F64 f) {
            return formatDouble(f.value());
        }
        return v.toString();
    }

    // ---------- explode ----------

    /**
     * Explode a {@code Value} column of {@link TsValue.Array} cells: each list
     * cell expands to one output row per element, with every other column's cell
     * repeated for each element. A row whose {@code listCol} cell is an EMPTY
     * array (or a null / non-array cell) is DROPPED, matching Polars
     * {@code explode} and DuckDB {@code UNNEST}.
     *
     * <p>The exploded column is an f64 {@link TsArray} when its elements are
     * numeric, else a {@code Str} array of the elements' string rendering; other
     * columns keep their type. Output order is input-row-major, then element order
     * within a row.
     */
    public static TsReshapeResult explode(TsDataFrame frame, String listCol) {
        FrameColumns fc = frameColumns(frame);
        List<String> names = fc.names();
        List<TsArray> columns = fc.columns();
        int listI = resolve(names, listCol);
        int nrows = nrowsOf(columns);
        List<Optional<TsValue>> listCells = frameValueCells(frame).get(listCol);

        List<List<TsValue>> rowElems = new ArrayList<>(nrows);
        for (int r = 0; r < nrows; r++) {
            List<TsValue> elems = new ArrayList<>();
            if (listCells != null) {
                Optional<TsValue> cell = listCells.get(r);
                if (cell.isPresent() && cell.get() instanceof TsValue.Array a) {
                    elems = new ArrayList<>(a.value());
                }
            }
            rowElems.add(elems);
        }

        int outRows = 0;
        boolean allNumeric = true;
        for (List<TsValue> elems : rowElems) {
            outRows += elems.size();
            for (TsValue e : elems) {
                if (valueAsDouble(e).isEmpty()) {
                    allNumeric = false;
                }
            }
        }
        TsDataType explodedType = allNumeric ? TsDataType.F64 : TsDataType.STR;

        List<String> outNames = new ArrayList<>(columns.size());
        List<TsArray> outCols = new ArrayList<>(columns.size());
        for (int ci = 0; ci < columns.size(); ci++) {
            if (ci == listI) {
                ArrayBuilder builder = ArrayBuilder.forType(explodedType, outRows);
                for (List<TsValue> elems : rowElems) {
                    for (TsValue e : elems) {
                        Optional<TsValue> cell = explodedType == TsDataType.STR
                                ? Optional.of(TsValue.ofString(stringifyCell(e)))
                                : valueAsDouble(e).map(TsValue::ofDouble);
                        builder.push(cell);
                    }
                }
                outNames.add(names.get(ci));
                outCols.add(builder.finish());
            } else {
                TsArray src = columns.get(ci);
                ArrayBuilder builder = ArrayBuilder.forType(src.dataType(), outRows);
                for (int r = 0; r < nrows; r++) {
                    Optional<TsValue> cell = src.get(r);
                    int reps = rowElems.get(r).size();
                    for (int j = 0; j < reps; j++) {
                        builder.push(cell);
                    }
                }
                outNames.add(names.get(ci));
                outCols.add(builder.finish());
            }
        }

        return new TsReshapeResult(outNames, outCols, outRows);
    }

    // ---------- concatenation ----------

    /**
     * Row concatenation: every row of {@code a} followed by every row of
     * {@code b}. Both frames must carry the same column names in the same order;
     * otherwise {@link TsReshapeException}. A cell missing in either input stays
     * missing.
     */
    public static TsReshapeResult vstack(TsDataFrame a, TsDataFrame b) {
        FrameColumns fa = frameColumns(a);
        FrameColumns fb = frameColumns(b);
        if (!fa.names().equals(fb.names())) {
            throw TsReshapeException.schemaMismatch(fa.names(), fb.names());
        }
        int aRows = nrowsOf(fa.columns());
        int bRows = nrowsOf(fb.columns());
        int total = aRows + bRows;

        List<String> outNames = new ArrayList<>(fa.names());
        List<TsArray> outCols = new ArrayList<>(fa.columns().size());
        for (int ci = 0; ci < fa.columns().size(); ci++) {
            TsArray ac = fa.columns().get(ci);
            TsArray bc = fb.columns().get(ci);
            ArrayBuilder builder = ArrayBuilder.forType(ac.dataType(), total);
            for (int r = 0; r < aRows; r++) {
                builder.push(ac.get(r));
            }
            for (int r = 0; r < bRows; r++) {
                builder.push(bc.get(r));
            }
            outCols.add(builder.finish());
        }
        return new TsReshapeResult(outNames, outCols, total);
    }

    /**
     * Column concatenation: the columns of {@code a} followed by the columns of
     * {@code b}, over a shared row axis. Both frames must have the same row count;
     * otherwise {@link TsReshapeException}. A column name carried by both inputs is
     * suffixed {@code _a} / {@code _b}.
     */
    public static TsReshapeResult hstack(TsDataFrame a, TsDataFrame b) {
        FrameColumns fa = frameColumns(a);
        FrameColumns fb = frameColumns(b);
        int aRows = nrowsOf(fa.columns());
        int bRows = nrowsOf(fb.columns());
        if (aRows != bRows) {
            throw TsReshapeException.rowCountMismatch(aRows, bRows);
        }
        List<String> aNames = fa.names();
        List<String> bNames = fb.names();

        List<String> outNames = new ArrayList<>(aNames.size() + bNames.size());
        List<TsArray> outCols = new ArrayList<>(aNames.size() + bNames.size());
        for (int ci = 0; ci < aNames.size(); ci++) {
            String name = aNames.get(ci);
            outNames.add(bNames.contains(name) ? name + "_a" : name);
            outCols.add(fa.columns().get(ci));
        }
        for (int ci = 0; ci < bNames.size(); ci++) {
            String name = bNames.get(ci);
            outNames.add(aNames.contains(name) ? name + "_b" : name);
            outCols.add(fb.columns().get(ci));
        }
        return new TsReshapeResult(outNames, outCols, aRows);
    }

    // ---------- row set-ops ----------

    // A row encoded for set membership: the per-cell typed token (or null for a
    // missing cell). Equality / hashing on the tuple, so f64 compares by bits, Str
    // by value, and a Str "3" never equals a numeric 3.
    private static final class RowKey {
        final CellKey[] cells;

        RowKey(CellKey[] cells) {
            this.cells = cells;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof RowKey k && java.util.Arrays.equals(cells, k.cells);
        }

        @Override
        public int hashCode() {
            return java.util.Arrays.hashCode(cells);
        }
    }

    private static RowKey rowKey(List<TsArray> columns, int r) {
        CellKey[] cells = new CellKey[columns.size()];
        for (int ci = 0; ci < columns.size(); ci++) {
            cells[ci] = cellKey(columns.get(ci), r);
        }
        return new RowKey(cells);
    }

    // A (source columns, row index) pick for a set-op output, in pick order.
    private record Pick(List<TsArray> cols, int row) {}

    private static TsReshapeResult assembleRows(
            List<String> names, List<TsArray> template, List<Pick> picks) {
        List<ArrayBuilder> builders = new ArrayList<>(template.size());
        for (TsArray c : template) {
            builders.add(ArrayBuilder.forType(c.dataType(), picks.size()));
        }
        for (Pick p : picks) {
            for (int ci = 0; ci < builders.size(); ci++) {
                builders.get(ci).push(p.cols().get(ci).get(p.row()));
            }
        }
        List<TsArray> outCols = new ArrayList<>(builders.size());
        for (ArrayBuilder b : builders) {
            outCols.add(b.finish());
        }
        return new TsReshapeResult(new ArrayList<>(names), outCols, picks.size());
    }

    private static void requireSameSchema(List<String> aNames, List<String> bNames) {
        if (!aNames.equals(bNames)) {
            throw TsReshapeException.schemaMismatch(aNames, bNames);
        }
    }

    /**
     * Distinct rows present in either {@code a} or {@code b}, treating each row as
     * a typed cell tuple. Both frames must share the same schema. Output order:
     * distinct rows of {@code a} in input order, then rows of {@code b} not already
     * seen, in input order.
     */
    public static TsReshapeResult union(TsDataFrame a, TsDataFrame b) {
        FrameColumns fa = frameColumns(a);
        FrameColumns fb = frameColumns(b);
        requireSameSchema(fa.names(), fb.names());

        Set<RowKey> seen = new HashSet<>();
        List<Pick> picks = new ArrayList<>();
        for (int r = 0; r < nrowsOf(fa.columns()); r++) {
            if (seen.add(rowKey(fa.columns(), r))) {
                picks.add(new Pick(fa.columns(), r));
            }
        }
        for (int r = 0; r < nrowsOf(fb.columns()); r++) {
            if (seen.add(rowKey(fb.columns(), r))) {
                picks.add(new Pick(fb.columns(), r));
            }
        }
        return assembleRows(fa.names(), fa.columns(), picks);
    }

    /**
     * Distinct rows present in BOTH {@code a} and {@code b}. Same schema required.
     * Output order: the qualifying distinct rows of {@code a} in input order.
     */
    public static TsReshapeResult intersect(TsDataFrame a, TsDataFrame b) {
        FrameColumns fa = frameColumns(a);
        FrameColumns fb = frameColumns(b);
        requireSameSchema(fa.names(), fb.names());

        Set<RowKey> bSet = new HashSet<>();
        for (int r = 0; r < nrowsOf(fb.columns()); r++) {
            bSet.add(rowKey(fb.columns(), r));
        }
        Set<RowKey> emitted = new HashSet<>();
        List<Pick> picks = new ArrayList<>();
        for (int r = 0; r < nrowsOf(fa.columns()); r++) {
            RowKey key = rowKey(fa.columns(), r);
            if (bSet.contains(key) && emitted.add(key)) {
                picks.add(new Pick(fa.columns(), r));
            }
        }
        return assembleRows(fa.names(), fa.columns(), picks);
    }

    /**
     * Distinct rows present in {@code a} but NOT in {@code b}. Same schema
     * required. Output order: the qualifying distinct rows of {@code a} in input
     * order.
     */
    public static TsReshapeResult except(TsDataFrame a, TsDataFrame b) {
        FrameColumns fa = frameColumns(a);
        FrameColumns fb = frameColumns(b);
        requireSameSchema(fa.names(), fb.names());

        Set<RowKey> bSet = new HashSet<>();
        for (int r = 0; r < nrowsOf(fb.columns()); r++) {
            bSet.add(rowKey(fb.columns(), r));
        }
        Set<RowKey> emitted = new HashSet<>();
        List<Pick> picks = new ArrayList<>();
        for (int r = 0; r < nrowsOf(fa.columns()); r++) {
            RowKey key = rowKey(fa.columns(), r);
            if (!bSet.contains(key) && emitted.add(key)) {
                picks.add(new Pick(fa.columns(), r));
            }
        }
        return assembleRows(fa.names(), fa.columns(), picks);
    }
}
