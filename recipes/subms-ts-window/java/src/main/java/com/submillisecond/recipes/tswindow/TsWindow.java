package com.submillisecond.recipes.tswindow;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.Eval;
import com.submillisecond.recipes.tsexpr.TsArray;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsexpr.TsExprException;

/**
 * SQL-style window functions over a heterogeneous {@link TsDataFrame},
 * partitioned by ANY typed key column. Each function partitions the frame's
 * union-of-timestamps rows by a TUPLE of TYPED key cells (a {@code Str} symbol,
 * an {@code I64} venue id, an {@code F64}, a {@code Bool}), orders the rows
 * inside each partition by an order-by column (default: the frame's row/ts
 * order), then applies a per-partition transform and scatters the result back
 * onto the original row axis. The output is a typed {@link TsArray} the same
 * length as the frame's aligned row axis - exactly what {@code subms-ts-expr}'s
 * {@code Eval.eval} produces, so a window output composes straight back into
 * further expression evaluation as a derived column.
 *
 * <p>The shapes mirror SQL's {@code OVER (PARTITION BY k ORDER BY o)}:
 *
 * <ul>
 *   <li>{@link #lag} / {@link #lead} - shift a column within each partition by
 *       {@code n} rows; cells that fall off the partition head/tail are null.
 *       Works for ANY column type - the result array matches the input column's
 *       type.
 *   <li>{@link #rowNumber} - 1..k position inside each partition ({@code I64}).
 *   <li>{@link #rank} / {@link #denseRank} - order-sensitive ranks; ties share a
 *       rank ({@code I64}).
 *   <li>{@link #cumsum} / {@link #cumprod} / {@link #cummin} / {@link #cummax} -
 *       running reductions inside each partition, in order-by order (numeric
 *       {@code F64}/{@code I64} input, {@code F64} result).
 *   <li>{@link #over} - evaluate a {@link TsExpr} aggregation over each
 *       partition's sub-frame and broadcast the scalar back to every row of that
 *       partition (the SQL {@code agg() OVER (PARTITION BY k)} shape).
 * </ul>
 *
 * <h2>Typed partition keys</h2>
 * The partition key is the TUPLE of the partition columns' TYPED cells at a row.
 * Partitioning by a {@code Str} symbol column is the headline case - a
 * {@code lag} over {@code px} partitioned by {@code symbol} is the per-symbol
 * previous price, with no pre-encoding step.
 *
 * <h2>Validity model</h2>
 * Window outputs carry a validity bitmap for undefined cells. A {@code lag(1)}
 * at a partition's first row has no predecessor, so that cell is invalid, not
 * zero. A running reduction whose input cell is itself invalid skips that cell
 * (the running state carries forward; the output is invalid only until a valid
 * input has been folded in). This is the DERIVED-null model {@code subms-ts-expr}
 * uses, distinct from {@code TsSeries}' no-null-on-ingest invariant.
 *
 * <p>Byte-equivalent behaviour to the Rust sibling's {@code subms_ts_window}
 * crate: same partition + order model, same validity model, modulo case style.
 */
public final class TsWindow {

    private TsWindow() {}

    // ---------- typed partition-key cells ----------

    // A typed, hashable partition-key cell. A DoubleKey keys on the bit pattern
    // of its f64 (with -0.0 normalised to 0.0) so equal values land in one
    // partition; a stored f64 is always finite (a series rejects non-finite on
    // ingest). A NullKey tags the slot index that was missing (or held a
    // non-hashable cell), so two distinct missing keys never collide with each
    // other or with a present key.
    private sealed interface KeyCell
            permits KeyCell.LongKey, KeyCell.DoubleKey, KeyCell.BoolKey, KeyCell.StrKey,
                    KeyCell.NullKey {

        record LongKey(long value) implements KeyCell {}

        record DoubleKey(long bits) implements KeyCell {}

        record BoolKey(boolean value) implements KeyCell {}

        record StrKey(String value) implements KeyCell {}

        record NullKey(int slot) implements KeyCell {}

        static KeyCell from(Optional<TsValue> cell, int slot) {
            if (cell.isPresent()) {
                TsValue v = cell.get();
                if (v instanceof TsValue.I64 x) {
                    return new LongKey(x.value());
                }
                if (v instanceof TsValue.F64 x) {
                    return new DoubleKey(Double.doubleToLongBits(normaliseZero(x.value())));
                }
                if (v instanceof TsValue.Bool x) {
                    return new BoolKey(x.value());
                }
                if (v instanceof TsValue.Str x) {
                    return new StrKey(x.value());
                }
            }
            // A missing or non-hashable cell (bytes / map / array / null) cannot
            // index a partition, so it lands in the slot's null bucket.
            return new NullKey(slot);
        }
    }

    // Normalise -0.0 to +0.0 so the two zeros share a bit pattern (and a
    // partition), matching value equality.
    private static double normaliseZero(double v) {
        return v == 0.0 ? 0.0 : v;
    }

    // The element type of a column / array.
    private enum Kind {
        F64, I64, BOOL, STR
    }

    // ---------- row axis ----------

    // The materialised row axis of a frame: column names + per-column dense
    // optional-cell lists over the union-of-timestamps rows. Built once so the
    // partition pass and every per-partition scan share it.
    private static final class RowAxis {
        final List<String> names;
        final List<List<Optional<TsValue>>> columns;
        final int nrows;

        RowAxis(List<String> names, List<List<Optional<TsValue>>> columns, int nrows) {
            this.names = names;
            this.columns = columns;
            this.nrows = nrows;
        }

        int indexOf(String name) {
            return names.indexOf(name);
        }

        int require(String name) {
            int i = indexOf(name);
            if (i < 0) {
                throw TsWindowException.unknownColumn(name);
            }
            return i;
        }

        static RowAxis build(TsDataFrame frame) {
            List<String> names = frame.columnNames();
            int nslots = names.size();
            List<TsDataFrame.Row> rows = frame.aligned();
            int nrows = rows.size();

            List<List<Optional<TsValue>>> columns = new ArrayList<>(nslots);
            for (int s = 0; s < nslots; s++) {
                columns.add(new ArrayList<>(nrows));
            }
            for (TsDataFrame.Row row : rows) {
                List<Optional<TsValue>> rv = row.values();
                for (int s = 0; s < nslots; s++) {
                    columns.get(s).add(s < rv.size() ? rv.get(s) : Optional.empty());
                }
            }
            return new RowAxis(names, columns, nrows);
        }
    }

    // The shared partition + order plan: every row of the aligned axis grouped
    // into a partition (first-seen key order), each partition's row indices
    // sorted by the order-by column (stable on ties). nrows is carried so callers
    // can size their output.
    private record Plan(int nrows, List<int[]> partitions) {}

    private static Plan plan(RowAxis axis, String[] partitionBy, String orderBy) {
        int[] keyIdx = new int[partitionBy.length];
        for (int i = 0; i < partitionBy.length; i++) {
            keyIdx[i] = axis.require(partitionBy[i]);
        }
        int orderIdx = orderBy != null ? axis.require(orderBy) : -1;

        // Group rows into partitions in first-seen key order. The map keys on the
        // typed key tuple; partition order is arrival order, which keeps the
        // single-partition and stable cases intuitive.
        Map<List<KeyCell>, Integer> index = new HashMap<>();
        List<List<Integer>> building = new ArrayList<>();
        for (int row = 0; row < axis.nrows; row++) {
            List<KeyCell> key = new ArrayList<>(keyIdx.length);
            for (int slot = 0; slot < keyIdx.length; slot++) {
                key.add(KeyCell.from(axis.columns.get(keyIdx[slot]).get(row), slot));
            }
            Integer pidx = index.get(key);
            if (pidx == null) {
                pidx = building.size();
                index.put(key, pidx);
                building.add(new ArrayList<>());
            }
            building.get(pidx).add(row);
        }

        List<int[]> partitions = new ArrayList<>(building.size());
        for (List<Integer> part : building) {
            if (orderIdx >= 0) {
                final List<Optional<TsValue>> order = axis.columns.get(orderIdx);
                part.sort((a, b) -> Double.compare(orderKey(order.get(a)), orderKey(order.get(b))));
            }
            int[] arr = new int[part.size()];
            for (int i = 0; i < arr.length; i++) {
                arr[i] = part.get(i);
            }
            partitions.add(arr);
        }
        return new Plan(axis.nrows, partitions);
    }

    // A numeric order key. A null order-by value sorts first; a non-numeric
    // present value also sorts first (its order is the row's arrival order,
    // preserved by the stable sort). Double.compare gives the same IEEE total
    // ordering Rust's total_cmp does, deterministically.
    private static double orderKey(Optional<TsValue> cell) {
        if (cell.isPresent()) {
            TsValue v = cell.get();
            if (v instanceof TsValue.F64 x) {
                return x.value();
            }
            if (v instanceof TsValue.I64 x) {
                return (double) x.value();
            }
            if (v instanceof TsValue.Bool x) {
                return x.value() ? 1.0 : 0.0;
            }
        }
        return Double.NEGATIVE_INFINITY;
    }

    // A numeric view of a cell (F64 / I64 widened to double), for the running
    // reductions. null for a missing or non-numeric cell.
    private static Double numericOf(Optional<TsValue> cell) {
        if (cell.isPresent()) {
            TsValue v = cell.get();
            if (v instanceof TsValue.F64 x) {
                return x.value();
            }
            if (v instanceof TsValue.I64 x) {
                return (double) x.value();
            }
        }
        return null;
    }

    // Pick a column's element type from its first present cell; an all-null
    // column defaults to F64 (an empty typed array a downstream numeric op can
    // still read).
    private static Kind columnKind(List<Optional<TsValue>> cells) {
        for (Optional<TsValue> c : cells) {
            if (c.isPresent()) {
                TsValue v = c.get();
                if (v instanceof TsValue.I64) {
                    return Kind.I64;
                }
                if (v instanceof TsValue.Bool) {
                    return Kind.BOOL;
                }
                if (v instanceof TsValue.Str) {
                    return Kind.STR;
                }
                if (v instanceof TsValue.F64) {
                    return Kind.F64;
                }
            }
        }
        return Kind.F64;
    }

    // ---------- shift functions ----------

    /**
     * {@code lag(column, n)} within each partition: each row takes the value
     * {@code n} positions earlier in its partition's order. The first {@code n}
     * rows of every partition have no predecessor and are invalid. Works for ANY
     * column type - the result {@link TsArray} matches the input column's type.
     */
    public static TsArray lag(TsDataFrame frame, String column, int n, String[] partitionBy) {
        return shift(frame, column, n, partitionBy, null);
    }

    /**
     * {@code lead(column, n)} within each partition: each row takes the value
     * {@code n} positions later. The last {@code n} rows of every partition have
     * no successor and are invalid. Works for ANY column type.
     */
    public static TsArray lead(TsDataFrame frame, String column, int n, String[] partitionBy) {
        return shift(frame, column, -n, partitionBy, null);
    }

    private static TsArray shift(
            TsDataFrame frame, String column, int offset, String[] partitionBy, String orderBy) {
        RowAxis axis = RowAxis.build(frame);
        int ci = axis.require(column);
        Plan plan = plan(axis, partitionBy, orderBy);

        // The source row for each output row: which axis row supplies its value,
        // or -1 when the offset falls off the partition head/tail.
        int[] src = new int[plan.nrows()];
        for (int i = 0; i < src.length; i++) {
            src[i] = -1;
        }
        for (int[] part : plan.partitions()) {
            int len = part.length;
            for (int pos = 0; pos < len; pos++) {
                int from = pos - offset;
                if (from >= 0 && from < len) {
                    src[part[pos]] = part[from];
                }
            }
        }
        return gather(axis.columns.get(ci), src);
    }

    // Build a typed array of the input column's type, taking each output row's
    // value from its source row (or null where the source is -1 or itself a null
    // cell).
    private static TsArray gather(List<Optional<TsValue>> input, int[] src) {
        Kind kind = columnKind(input);
        int n = src.length;
        switch (kind) {
            case I64 -> {
                long[] values = new long[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (src[i] >= 0 && input.get(src[i]).orElse(null) instanceof TsValue.I64 x) {
                        values[i] = x.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.I64(values, valid);
            }
            case BOOL -> {
                boolean[] values = new boolean[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (src[i] >= 0 && input.get(src[i]).orElse(null) instanceof TsValue.Bool x) {
                        values[i] = x.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.Bool(values, valid);
            }
            case STR -> {
                String[] values = new String[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (src[i] >= 0 && input.get(src[i]).orElse(null) instanceof TsValue.Str x) {
                        values[i] = x.value();
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
                    if (src[i] >= 0 && input.get(src[i]).orElse(null) instanceof TsValue.F64 x) {
                        values[i] = x.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.F64(values, valid);
            }
        }
    }

    // ---------- numbering / ranking ----------

    /**
     * {@code row_number()} within each partition: 1..k in order-by order. Always
     * fully valid - every row has a position. The result is an {@code I64} array.
     */
    public static TsArray rowNumber(TsDataFrame frame, String[] partitionBy, String orderBy) {
        RowAxis axis = RowAxis.build(frame);
        Plan plan = plan(axis, partitionBy, orderBy);
        long[] values = new long[plan.nrows()];
        boolean[] valid = new boolean[plan.nrows()];
        for (int[] part : plan.partitions()) {
            for (int pos = 0; pos < part.length; pos++) {
                values[part[pos]] = pos + 1;
                valid[part[pos]] = true;
            }
        }
        return new TsArray.I64(values, valid);
    }

    /**
     * {@code rank()} within each partition: order-sensitive rank where ties share
     * the lowest rank and the next distinct value skips the gap (1, 1, 3, ...).
     * Ranks are by the order-by column. The result is an {@code I64} array.
     */
    public static TsArray rank(TsDataFrame frame, String[] partitionBy, String orderBy) {
        return ranked(frame, partitionBy, orderBy, false);
    }

    /**
     * {@code dense_rank()} within each partition: like {@link #rank} but
     * consecutive distinct values do not skip (1, 1, 2, ...). The result is an
     * {@code I64} array.
     */
    public static TsArray denseRank(TsDataFrame frame, String[] partitionBy, String orderBy) {
        return ranked(frame, partitionBy, orderBy, true);
    }

    private static TsArray ranked(
            TsDataFrame frame, String[] partitionBy, String orderBy, boolean dense) {
        RowAxis axis = RowAxis.build(frame);
        int oi = axis.require(orderBy);
        Plan plan = plan(axis, partitionBy, orderBy);
        long[] values = new long[plan.nrows()];
        boolean[] valid = new boolean[plan.nrows()];

        for (int[] part : plan.partitions()) {
            long rankValue = 0L;
            boolean hasPrev = false;
            double prev = 0.0;
            for (int idx = 0; idx < part.length; idx++) {
                int row = part[idx];
                double key = orderKey(axis.columns.get(oi).get(row));
                boolean isNew = !hasPrev || Double.compare(key, prev) != 0;
                if (isNew) {
                    // The gap-skipping rank jumps to the 1-based position; the
                    // dense rank advances by one.
                    rankValue = dense ? rankValue + 1 : idx + 1;
                }
                values[row] = rankValue;
                valid[row] = true;
                prev = key;
                hasPrev = true;
            }
        }
        return new TsArray.I64(values, valid);
    }

    // ---------- running reductions ----------

    private enum RunOp {
        SUM, PROD, MIN, MAX
    }

    private static TsArray cumulative(
            TsDataFrame frame, String column, String[] partitionBy, String orderBy, RunOp op) {
        RowAxis axis = RowAxis.build(frame);
        int ci = axis.require(column);
        Kind kind = columnKind(axis.columns.get(ci));
        if (kind != Kind.F64 && kind != Kind.I64) {
            throw TsWindowException.notNumeric(column);
        }
        Plan plan = plan(axis, partitionBy, orderBy);
        double[] values = new double[plan.nrows()];
        boolean[] valid = new boolean[plan.nrows()];

        for (int[] part : plan.partitions()) {
            boolean hasAcc = false;
            double acc = 0.0;
            for (int row : part) {
                Double v = numericOf(axis.columns.get(ci).get(row));
                if (v != null) {
                    if (!hasAcc) {
                        acc = v;
                        hasAcc = true;
                    } else {
                        acc = switch (op) {
                            case SUM -> acc + v;
                            case PROD -> acc * v;
                            case MIN -> Math.min(acc, v);
                            case MAX -> Math.max(acc, v);
                        };
                    }
                }
                // The running state carries across nulls; the output cell is
                // valid once at least one valid input has been folded in.
                if (hasAcc) {
                    values[row] = acc;
                    valid[row] = true;
                }
            }
        }
        return new TsArray.F64(values, valid);
    }

    /**
     * Running sum within each partition, in order-by order. The column must be
     * numeric ({@code F64} / {@code I64}); the result is an {@code F64} array.
     */
    public static TsArray cumsum(
            TsDataFrame frame, String column, String[] partitionBy, String orderBy) {
        return cumulative(frame, column, partitionBy, orderBy, RunOp.SUM);
    }

    /** Running product within each partition, in order-by order. Numeric only. */
    public static TsArray cumprod(
            TsDataFrame frame, String column, String[] partitionBy, String orderBy) {
        return cumulative(frame, column, partitionBy, orderBy, RunOp.PROD);
    }

    /** Running minimum within each partition, in order-by order. Numeric only. */
    public static TsArray cummin(
            TsDataFrame frame, String column, String[] partitionBy, String orderBy) {
        return cumulative(frame, column, partitionBy, orderBy, RunOp.MIN);
    }

    /** Running maximum within each partition, in order-by order. Numeric only. */
    public static TsArray cummax(
            TsDataFrame frame, String column, String[] partitionBy, String orderBy) {
        return cumulative(frame, column, partitionBy, orderBy, RunOp.MAX);
    }

    // ---------- over() ----------

    /**
     * {@code aggExpr OVER (PARTITION BY k)}: evaluate the aggregation
     * {@code aggExpr} over each partition's sub-frame and broadcast the resulting
     * scalar back to every row of that partition (the SQL
     * {@code agg() OVER (PARTITION BY k)} shape). {@code aggExpr} must be a
     * top-level {@link TsExpr.Agg}. The result array's type is the reduction's
     * type ({@code Sum}/{@code Mean} -> {@code F64}, {@code Count} -> {@code I64},
     * {@code Min}/{@code Max} -> the operand's type), uniform across partitions. A
     * {@code NaN} reduction (mean of an empty / all-null partition) surfaces as a
     * null cell, never a NaN a downstream consumer must special-case.
     */
    public static TsArray over(TsDataFrame frame, TsExpr aggExpr, String[] partitionBy) {
        if (!(aggExpr instanceof TsExpr.Agg)) {
            throw TsWindowException.notAnAggregation();
        }
        RowAxis axis = RowAxis.build(frame);
        Plan plan = plan(axis, partitionBy, null);

        // Reduce each partition to a scalar, recording the partition each row sits
        // in so the scalars scatter back across the row axis.
        List<TsValue> scalars = new ArrayList<>(plan.partitions().size());
        int[] partOf = new int[plan.nrows()];
        List<int[]> partitions = plan.partitions();
        for (int pidx = 0; pidx < partitions.size(); pidx++) {
            int[] part = partitions.get(pidx);
            TsDataFrame sub = subFrame(axis, part);
            try {
                scalars.add(Eval.evalScalar(aggExpr, sub));
            } catch (TsExprException e) {
                throw fromExpr(e);
            }
            for (int row : part) {
                partOf[row] = pidx;
            }
        }
        return broadcast(scalars, partOf);
    }

    // Build a sub-frame holding only the partition's rows of every column,
    // re-emitted at synthetic monotonic timestamps so the evaluator's
    // union-of-timestamps row axis lines up 1:1 with the partition's rows in
    // partition order. A missing (null) cell is not pushed, so the reduction sees
    // the same validity the partition's slice of the parent axis has.
    private static TsDataFrame subFrame(RowAxis axis, int[] rows) {
        TsDataFrame f = new TsDataFrame();
        for (int ci = 0; ci < axis.names.size(); ci++) {
            List<Optional<TsValue>> cells = axis.columns.get(ci);
            TsColumn col = switch (columnKind(cells)) {
                case I64 -> {
                    TsSeriesL s = new TsSeriesL();
                    long synth = 0;
                    for (int r : rows) {
                        if (cells.get(r).orElse(null) instanceof TsValue.I64 x) {
                            s.push(synth, x.value());
                        }
                        synth++;
                    }
                    yield new TsColumn.I64(s);
                }
                case BOOL -> {
                    TsSeries<Boolean> s = new TsSeries<>();
                    long synth = 0;
                    for (int r : rows) {
                        if (cells.get(r).orElse(null) instanceof TsValue.Bool x) {
                            s.push(synth, x.value());
                        }
                        synth++;
                    }
                    yield new TsColumn.Bool(s);
                }
                case STR -> {
                    TsSeries<String> s = new TsSeries<>();
                    long synth = 0;
                    for (int r : rows) {
                        if (cells.get(r).orElse(null) instanceof TsValue.Str x) {
                            s.push(synth, x.value());
                        }
                        synth++;
                    }
                    yield new TsColumn.Str(s);
                }
                default -> {
                    TsSeriesD s = new TsSeriesD();
                    long synth = 0;
                    for (int r : rows) {
                        if (cells.get(r).orElse(null) instanceof TsValue.F64 x) {
                            s.push(synth, x.value());
                        }
                        synth++;
                    }
                    yield new TsColumn.F64(s);
                }
            };
            // axis names are distinct (frame columns are), so no dup.
            f.pushColumn(axis.names.get(ci), col);
        }
        return f;
    }

    // Scatter each partition's scalar across its rows. The result type is the
    // scalars' uniform type; a Null / NaN scalar broadcasts as an invalid cell.
    private static TsArray broadcast(List<TsValue> scalars, int[] partOf) {
        Kind kind = Kind.F64;
        for (TsValue v : scalars) {
            if (v instanceof TsValue.F64 d && Double.isNaN(d.value())) {
                continue;
            }
            if (v instanceof TsValue.Null) {
                continue;
            }
            if (v instanceof TsValue.I64) {
                kind = Kind.I64;
                break;
            }
            if (v instanceof TsValue.Bool) {
                kind = Kind.BOOL;
                break;
            }
            if (v instanceof TsValue.Str) {
                kind = Kind.STR;
                break;
            }
            if (v instanceof TsValue.F64) {
                kind = Kind.F64;
                break;
            }
        }

        int n = partOf.length;
        switch (kind) {
            case I64 -> {
                long[] values = new long[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (scalars.get(partOf[i]) instanceof TsValue.I64 x) {
                        values[i] = x.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.I64(values, valid);
            }
            case BOOL -> {
                boolean[] values = new boolean[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (scalars.get(partOf[i]) instanceof TsValue.Bool x) {
                        values[i] = x.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.Bool(values, valid);
            }
            case STR -> {
                String[] values = new String[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (scalars.get(partOf[i]) instanceof TsValue.Str x) {
                        values[i] = x.value();
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
                    TsValue v = scalars.get(partOf[i]);
                    Double f = null;
                    if (v instanceof TsValue.F64 x) {
                        f = x.value();
                    } else if (v instanceof TsValue.I64 x) {
                        f = (double) x.value();
                    }
                    if (f != null && !Double.isNaN(f)) {
                        values[i] = f;
                        valid[i] = true;
                    }
                }
                return new TsArray.F64(values, valid);
            }
        }
    }

    // Map a pure-compute evaluator error onto the window engine's structural
    // error vocabulary, mirroring the Rust From<TsExprError> impl.
    private static TsWindowException fromExpr(TsExprException e) {
        return switch (e.kind()) {
            case UNKNOWN_COLUMN -> TsWindowException.unknownColumn(e.getMessage());
            case TYPE_MISMATCH -> TsWindowException.notNumeric(e.getMessage());
            case NOT_SCALAR -> TsWindowException.notAnAggregation();
        };
    }
}
