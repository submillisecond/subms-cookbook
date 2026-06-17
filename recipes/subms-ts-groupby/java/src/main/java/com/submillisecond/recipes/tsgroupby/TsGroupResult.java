package com.submillisecond.recipes.tsgroupby;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The result of an analytical operator: a positional named-{@link TsArray}
 * table, one row per group, key columns first then aggregated columns. Rows are
 * in deterministic (key-sorted) order. This is the shared result shape across
 * the group-by / value_counts / unique surface. Mirrors the Rust sibling's
 * {@code TsGroupResult}.
 */
public final class TsGroupResult {

    /** One named column of the result table. */
    public record Column(String name, TsArray array) {}

    private final List<Column> columns;
    private final int nrows;

    TsGroupResult(List<Column> columns, int nrows) {
        this.columns = columns;
        this.nrows = nrows;
    }

    /** Number of result rows (one per group). */
    public int nrows() {
        return nrows;
    }

    /** Number of columns (key columns + aggregated columns). */
    public int ncols() {
        return columns.size();
    }

    /** The column names, in order (key columns then aggregated columns). */
    public List<String> columnNames() {
        List<String> out = new ArrayList<>(columns.size());
        for (Column c : columns) {
            out.add(c.name());
        }
        return out;
    }

    /** The typed array of column {@code name}, or empty if absent. */
    public Optional<TsArray> column(String name) {
        for (Column c : columns) {
            if (c.name().equals(name)) {
                return Optional.of(c.array());
            }
        }
        return Optional.empty();
    }

    /**
     * The boxed value of column {@code name} at {@code row}, or empty if the
     * column is absent or that cell is null.
     */
    public Optional<TsValue> value(String name, int row) {
        Optional<TsArray> a = column(name);
        if (a.isEmpty() || row < 0 || row >= a.get().len()) {
            return Optional.empty();
        }
        return a.get().get(row);
    }
}
