package com.submillisecond.recipes.tsjoin;

import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The result of a join: ordered named columns, all of the same length
 * ({@link #nrows}). Missing cells (the unmatched side of an outer / left /
 * right join) carry an unset validity bit in their {@link TsArray}; read them
 * with {@link TsArray#get} (empty on a null) or coalesce with
 * {@link TsArray#fillNull}.
 */
public final class TsJoinResult {

    private final List<String> names;
    private final List<TsArray> columns;
    private final int nrows;

    TsJoinResult(List<String> names, List<TsArray> columns, int nrows) {
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

    /**
     * Column names in output order. Key columns first (once, unsuffixed), then
     * left payload, then right payload; collisions carry {@code _left} /
     * {@code _right} suffixes.
     */
    public List<String> columnNames() {
        return names;
    }

    /** A column by its (possibly suffixed) output name. */
    public Optional<TsArray> column(String name) {
        int i = names.indexOf(name);
        return i < 0 ? Optional.empty() : Optional.of(columns.get(i));
    }

    /** A column by output index. */
    public Optional<TsArray> columnAt(int i) {
        return i < 0 || i >= columns.size() ? Optional.empty() : Optional.of(columns.get(i));
    }
}
