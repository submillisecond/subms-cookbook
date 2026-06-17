package com.submillisecond.recipes.tsgroupby;

import java.util.ArrayList;
import java.util.Comparator;
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
import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * Typed, multi-key group-by with multi-aggregation over a heterogeneous
 * {@link TsDataFrame}, the defining Polars / DuckDB operation. The key columns
 * may be ANY typed column - a {@code Str} symbol, an {@code I64} id / date, an
 * {@code F64}, a {@code Bool} - and the group key is the TUPLE of those typed
 * cells. {@link #groupBy} builds a {@link TsGroupBy} partition, and
 * {@link TsGroupBy#agg} reduces a set of {@code TsExpr} aggregations per group
 * through the {@code subms-ts-expr} evaluator.
 *
 * <h2>Row model</h2>
 * A frame is a bag of named, per-column-typed series. We materialise its
 * union-of-timestamps row axis exactly as the evaluator does (via
 * {@code frame.aligned()}), so a row is a tuple of optional typed cells, one per
 * column. The key of a row is the tuple of the key columns' cells AT that row.
 *
 * <h2>Null-key policy</h2>
 * A row whose key tuple contains ANY null/missing (or non-hashable) cell is
 * DROPPED - it does not form a group and contributes to no aggregate. The choice
 * is pinned by a test; callers who want a null bucket coalesce the key upstream
 * before grouping.
 *
 * <h2>Determinism</h2>
 * Output rows are sorted by the key tuple (lexicographic over the typed keys),
 * so the result is reproducible regardless of input row order or hash iteration
 * order.
 *
 * <p>Byte-equivalent behaviour to the Rust sibling's {@code subms-ts-groupby}
 * crate: same surface, same null-key policy, same deterministic group order,
 * modulo case style.
 */
public final class GroupBy {

    private GroupBy() {}

    /**
     * Partition {@code frame}'s rows by the tuple of TYPED values in
     * {@code keys}. A key column may be {@code Str}, {@code I64}, {@code F64}, or
     * {@code Bool}. Rows with a null / non-hashable cell in any key column are
     * dropped (see the class docs). Raises for an empty key list or an unknown
     * column.
     */
    public static TsGroupBy groupBy(TsDataFrame frame, String... keys) {
        if (keys.length == 0) {
            throw GroupByException.noKeys();
        }

        RowAxis axis = RowAxis.build(frame);
        int[] keyIdx = new int[keys.length];
        for (int i = 0; i < keys.length; i++) {
            keyIdx[i] = axis.indexOf(keys[i]);
            if (keyIdx[i] < 0) {
                throw GroupByException.unknownColumn(keys[i]);
            }
        }

        // Single-pass hash partition. The map keys on the typed key tuple; the
        // value is the index into groups, which accumulates the first-seen key.
        Map<List<KeyCell>, Integer> index = new HashMap<>();
        List<GroupSlot> groups = new ArrayList<>();
        for (int r = 0; r < axis.nrows; r++) {
            List<KeyCell> key = new ArrayList<>(keyIdx.length);
            boolean droppable = false;
            for (int ci : keyIdx) {
                KeyCell cell = KeyCell.from(axis.cells.get(ci).get(r));
                if (cell == null) {
                    droppable = true;
                    break;
                }
                key.add(cell);
            }
            if (droppable) {
                continue;
            }
            Integer slot = index.get(key);
            if (slot == null) {
                slot = groups.size();
                index.put(key, slot);
                groups.add(new GroupSlot(key));
            }
            groups.get(slot).rows.add(r);
        }

        groups.sort(GROUP_ORDER);

        List<String> keyNames = new ArrayList<>(keys.length);
        for (String k : keys) {
            keyNames.add(k);
        }
        return new TsGroupBy(keyNames, groups, axis);
    }

    /**
     * Group {@code column} by value and count occurrences, returning a
     * {@link TsGroupResult} with the key column plus a {@code count} ({@code I64})
     * column, sorted by DESCENDING count (ties broken by ascending key). The
     * analytical {@code Series.value_counts}. Null / non-hashable cells are not
     * counted.
     */
    public static TsGroupResult valueCounts(TsDataFrame frame, String column) {
        TsGroupBy gb = groupBy(frame, column);
        List<GroupSlot> slots = gb.groups();
        List<Integer> order = new ArrayList<>(slots.size());
        for (int g = 0; g < slots.size(); g++) {
            order.add(g);
        }
        order.sort((a, b) -> {
            int cmp = Integer.compare(slots.get(b).rows.size(), slots.get(a).rows.size());
            return cmp != 0 ? cmp : KeyCell.compare(slots.get(a).key, slots.get(b).key);
        });

        List<KeyCell> keyCells = new ArrayList<>(order.size());
        long[] counts = new long[order.size()];
        for (int i = 0; i < order.size(); i++) {
            GroupSlot s = slots.get(order.get(i));
            keyCells.add(s.key.get(0));
            counts[i] = s.rows.size();
        }

        List<TsGroupResult.Column> cols = new ArrayList<>(2);
        cols.add(new TsGroupResult.Column(column, keyArray(keyCells)));
        cols.add(new TsGroupResult.Column("count", new TsArray.I64(counts, TsArray.allTrue(counts.length))));
        return new TsGroupResult(cols, counts.length);
    }

    /**
     * The distinct key tuples present in {@code frame} over {@code columns}, as a
     * {@link TsGroupResult} of just the key columns, in deterministic
     * (key-sorted) order. Rows with a null in any of {@code columns} are dropped,
     * same as {@link #groupBy}. The analytical
     * {@code DataFrame.unique(subset=columns)}.
     */
    public static TsGroupResult unique(TsDataFrame frame, String... columns) {
        TsGroupBy gb = groupBy(frame, columns);
        List<GroupSlot> slots = gb.groups();
        List<String> keyNames = gb.keyNames();
        List<TsGroupResult.Column> cols = new ArrayList<>(keyNames.size());
        for (int ki = 0; ki < keyNames.size(); ki++) {
            List<KeyCell> cells = new ArrayList<>(slots.size());
            for (GroupSlot s : slots) {
                cells.add(s.key.get(ki));
            }
            cols.add(new TsGroupResult.Column(keyNames.get(ki), keyArray(cells)));
        }
        return new TsGroupResult(cols, slots.size());
    }

    /**
     * Reorder {@code frame}'s rows by {@code column}'s value, largest first, and
     * return the top {@code k} as a new {@link TsDataFrame} (every column
     * projected, at fresh synthetic monotonic timestamps). {@code column} may be
     * {@code F64} or {@code I64}. Rows with a null in {@code column} are excluded
     * from ranking. Ties broken by ascending original row index. The analytical
     * {@code DataFrame.top_k(k, by=column)}.
     */
    public static TsDataFrame topK(TsDataFrame frame, String column, int k) {
        RowAxis axis = RowAxis.build(frame);
        int ci = axis.indexOf(column);
        if (ci < 0) {
            throw GroupByException.unknownColumn(column);
        }

        List<Integer> ranked = new ArrayList<>();
        for (int r = 0; r < axis.nrows; r++) {
            if (numericOf(axis.cells.get(ci).get(r)) != null) {
                ranked.add(r);
            }
        }
        final int fci = ci;
        ranked.sort((a, b) -> {
            double va = numericOf(axis.cells.get(fci).get(a));
            double vb = numericOf(axis.cells.get(fci).get(b));
            int cmp = Double.compare(vb, va);
            return cmp != 0 ? cmp : Integer.compare(a, b);
        });
        if (ranked.size() > k) {
            ranked = ranked.subList(0, k);
        }
        return reorderFrame(axis, ranked);
    }

    /**
     * Sort {@code frame}'s rows by {@code columns} (lexicographic, multi-key) and
     * return a new {@link TsDataFrame} in the chosen order (every column
     * projected, at fresh synthetic monotonic timestamps). {@code ascending}
     * flips the direction for ALL keys. Rows with a null in a sort key sort last
     * (nulls-last). The analytical {@code DataFrame.sort(by=columns)}.
     */
    public static TsDataFrame sortBy(TsDataFrame frame, boolean ascending, String... columns) {
        if (columns.length == 0) {
            throw GroupByException.noKeys();
        }
        RowAxis axis = RowAxis.build(frame);
        int[] keyIdx = new int[columns.length];
        for (int i = 0; i < columns.length; i++) {
            keyIdx[i] = axis.indexOf(columns[i]);
            if (keyIdx[i] < 0) {
                throw GroupByException.unknownColumn(columns[i]);
            }
        }

        List<Integer> order = new ArrayList<>(axis.nrows);
        for (int r = 0; r < axis.nrows; r++) {
            order.add(r);
        }
        order.sort((a, b) -> {
            for (int ci : keyIdx) {
                KeyCell va = KeyCell.from(axis.cells.get(ci).get(a));
                KeyCell vb = KeyCell.from(axis.cells.get(ci).get(b));
                int cmp;
                if (va != null && vb != null) {
                    int base = va.compareTo(vb);
                    cmp = ascending ? base : -base;
                } else if (va != null) {
                    cmp = -1; // present sorts before null (nulls-last)
                } else if (vb != null) {
                    cmp = 1;
                } else {
                    cmp = 0;
                }
                if (cmp != 0) {
                    return cmp;
                }
            }
            return Integer.compare(a, b);
        });
        return reorderFrame(axis, order);
    }

    // ---------- internals shared with TsGroupBy ----------

    static final class GroupSlot {
        final List<KeyCell> key;
        final List<Integer> rows = new ArrayList<>();

        GroupSlot(List<KeyCell> key) {
            this.key = key;
        }
    }

    /**
     * The materialised row axis of a frame: column names + per-column dense
     * optional-cell lists over the union-of-timestamps rows. Built once so the
     * partition pass and every per-group sub-frame share it.
     */
    static final class RowAxis {
        final List<String> names;
        final List<List<Optional<TsValue>>> cells;
        final int nrows;

        RowAxis(List<String> names, List<List<Optional<TsValue>>> cells, int nrows) {
            this.names = names;
            this.cells = cells;
            this.nrows = nrows;
        }

        int indexOf(String name) {
            return names.indexOf(name);
        }

        static RowAxis build(TsDataFrame frame) {
            List<String> names = frame.columnNames();
            int nslots = names.size();
            List<TsDataFrame.Row> rows = frame.aligned();
            int nrows = rows.size();

            List<List<Optional<TsValue>>> cells = new ArrayList<>(nslots);
            for (int s = 0; s < nslots; s++) {
                cells.add(new ArrayList<>(nrows));
            }
            for (TsDataFrame.Row row : rows) {
                List<Optional<TsValue>> rv = row.values();
                for (int s = 0; s < nslots; s++) {
                    cells.get(s).add(s < rv.size() ? rv.get(s) : Optional.empty());
                }
            }
            return new RowAxis(names, cells, nrows);
        }
    }

    // Build a sub-frame column for a set of rows: re-emit the kept rows' values
    // at synthetic monotonic timestamps, preserving the parent column's type. A
    // column whose every kept cell is null still needs the right empty-typed
    // series so an Agg over it reduces under the correct type.
    static TsColumn buildColumn(List<Optional<TsValue>> cells, List<Integer> rows) {
        ValueKind kind = ValueKind.F64;
        for (Optional<TsValue> c : cells) {
            if (c.isPresent()) {
                kind = ValueKind.of(c.get());
                break;
            }
        }
        switch (kind) {
            case I64 -> {
                TsSeriesL s = new TsSeriesL();
                long synthTs = 0;
                for (int r : rows) {
                    Optional<TsValue> c = cells.get(r);
                    if (c.isPresent() && c.get() instanceof TsValue.I64 v) {
                        s.push(synthTs, v.value());
                    }
                    synthTs++;
                }
                return new TsColumn.I64(s);
            }
            case BOOL -> {
                TsSeries<Boolean> s = new TsSeries<>();
                long synthTs = 0;
                for (int r : rows) {
                    Optional<TsValue> c = cells.get(r);
                    if (c.isPresent() && c.get() instanceof TsValue.Bool v) {
                        s.push(synthTs, v.value());
                    }
                    synthTs++;
                }
                return new TsColumn.Bool(s);
            }
            case STR -> {
                TsSeries<String> s = new TsSeries<>();
                long synthTs = 0;
                for (int r : rows) {
                    Optional<TsValue> c = cells.get(r);
                    if (c.isPresent() && c.get() instanceof TsValue.Str v) {
                        s.push(synthTs, v.value());
                    }
                    synthTs++;
                }
                return new TsColumn.Str(s);
            }
            default -> {
                TsSeriesD s = new TsSeriesD();
                long synthTs = 0;
                for (int r : rows) {
                    Double d = numericOf(cells.get(r));
                    if (d != null) {
                        s.push(synthTs, d);
                    }
                    synthTs++;
                }
                return new TsColumn.F64(s);
            }
        }
    }

    private static TsDataFrame reorderFrame(RowAxis axis, List<Integer> order) {
        TsDataFrame f = new TsDataFrame();
        for (int ci = 0; ci < axis.names.size(); ci++) {
            f.pushColumn(axis.names.get(ci), buildColumn(axis.cells.get(ci), order));
        }
        return f;
    }

    // Build a fully-valid TsArray from typed group keys. Keys in one column share
    // a type; the first cell picks the variant; an empty set defaults to F64.
    static TsArray keyArray(List<KeyCell> cells) {
        if (cells.isEmpty()) {
            return new TsArray.F64(new double[0], new boolean[0]);
        }
        KeyCell first = cells.get(0);
        int n = cells.size();
        if (first instanceof KeyCell.LongKey) {
            long[] vs = new long[n];
            for (int i = 0; i < n; i++) {
                vs[i] = ((KeyCell.LongKey) cells.get(i)).value();
            }
            return new TsArray.I64(vs, TsArray.allTrue(n));
        } else if (first instanceof KeyCell.DoubleKey) {
            double[] vs = new double[n];
            for (int i = 0; i < n; i++) {
                vs[i] = ((KeyCell.DoubleKey) cells.get(i)).value();
            }
            return new TsArray.F64(vs, TsArray.allTrue(n));
        } else if (first instanceof KeyCell.BoolKey) {
            boolean[] vs = new boolean[n];
            for (int i = 0; i < n; i++) {
                vs[i] = ((KeyCell.BoolKey) cells.get(i)).value();
            }
            return new TsArray.Bool(vs, TsArray.allTrue(n));
        } else {
            String[] vs = new String[n];
            for (int i = 0; i < n; i++) {
                vs[i] = ((KeyCell.StrKey) cells.get(i)).value();
            }
            return new TsArray.Str(vs, TsArray.allTrue(n));
        }
    }

    // A numeric view of a cell (F64 / I64 widened to double), or null.
    static Double numericOf(Optional<TsValue> c) {
        if (c.isPresent()) {
            TsValue v = c.get();
            if (v instanceof TsValue.F64 d) {
                return d.value();
            }
            if (v instanceof TsValue.I64 l) {
                return (double) l.value();
            }
        }
        return null;
    }

    enum ValueKind {
        F64, I64, BOOL, STR;

        static ValueKind of(TsValue v) {
            if (v instanceof TsValue.I64) {
                return I64;
            }
            if (v instanceof TsValue.Bool) {
                return BOOL;
            }
            if (v instanceof TsValue.Str) {
                return STR;
            }
            return F64;
        }
    }

    // Lexicographic group order over the typed key tuple.
    static final Comparator<GroupSlot> GROUP_ORDER = (a, b) -> KeyCell.compare(a.key, b.key);
}
