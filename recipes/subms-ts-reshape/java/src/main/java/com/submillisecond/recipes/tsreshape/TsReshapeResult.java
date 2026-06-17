package com.submillisecond.recipes.tsreshape;

import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The result of a reshape: ordered named columns, all of the same length
 * ({@link #nrows}). Absent cells (an (index, column) pivot pair with no source
 * rows, or a melt over a column missing at a row) carry an unset validity bit;
 * read them with {@link TsArray#get} or coalesce with {@link TsArray#fillNull}.
 * The exact shape {@code subms-ts-join} / {@code subms-ts-groupby} produce, so a
 * reshape output drops straight back into the expression evaluator.
 */
public final class TsReshapeResult {

    private final List<String> names;
    private final List<TsArray> columns;
    private final int nrows;

    TsReshapeResult(List<String> names, List<TsArray> columns, int nrows) {
        this.names = names;
        this.columns = columns;
        this.nrows = nrows;
    }

    public int nrows() {
        return nrows;
    }

    public int ncols() {
        return columns.size();
    }

    public boolean isEmpty() {
        return nrows == 0;
    }

    /** Column names in output order. */
    public List<String> columnNames() {
        return names;
    }

    /** A column by its output name. */
    public Optional<TsArray> column(String name) {
        int i = names.indexOf(name);
        return i < 0 ? Optional.empty() : Optional.of(columns.get(i));
    }

    /** A column by output index. */
    public Optional<TsArray> columnAt(int i) {
        return i < 0 || i >= columns.size() ? Optional.empty() : Optional.of(columns.get(i));
    }
}
