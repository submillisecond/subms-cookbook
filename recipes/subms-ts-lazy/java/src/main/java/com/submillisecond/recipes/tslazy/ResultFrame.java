package com.submillisecond.recipes.tslazy;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsArray;

/**
 * The executed result: a ts axis plus named typed columns, kept in pipeline
 * (post-filter / post-sort) row order. Distinct from a {@link TsDataFrame}
 * because a sorted result's ts axis is not necessarily monotonic.
 *
 * <p>Byte-equivalent in shape to the Rust sibling's {@code ResultFrame}.
 */
public final class ResultFrame {

    private final long[] ts;
    private final List<String> names;
    private final List<TsArray> columns;

    ResultFrame(long[] ts, List<String> names, List<TsArray> columns) {
        this.ts = ts;
        this.names = names;
        this.columns = columns;
    }

    public int nrows() {
        return ts.length;
    }

    public int ncols() {
        return columns.size();
    }

    public boolean isEmpty() {
        return columns.isEmpty() || ts.length == 0;
    }

    public long[] ts() {
        return ts.clone();
    }

    public List<String> columnNames() {
        return List.copyOf(names);
    }

    /** The typed array for a named column, or empty if absent. */
    public Optional<TsArray> column(String name) {
        int i = names.indexOf(name);
        return i < 0 ? Optional.empty() : Optional.of(columns.get(i));
    }

    /** The boxed cell at {@code (row, column)}, empty if null or absent. */
    public Optional<TsValue> cell(int row, String name) {
        return column(name).flatMap(c -> c.get(row));
    }

    /**
     * Round-trip back to a {@link TsDataFrame}: order rows by ts (the frame's
     * no-out-of-order invariant) and gather each column's present cells into a
     * typed series. A null cell becomes a gap, which is the frame's null model.
     */
    public TsDataFrame intoDataFrame() {
        int n = ts.length;
        Integer[] order = new Integer[n];
        for (int i = 0; i < n; i++) {
            order[i] = i;
        }
        java.util.Arrays.sort(order, (a, b) -> Long.compare(ts[a], ts[b]));
        int[] ord = new int[n];
        for (int i = 0; i < n; i++) {
            ord[i] = order[i];
        }

        TsDataFrame out = new TsDataFrame();
        for (int c = 0; c < names.size(); c++) {
            out.withColumn(names.get(c), gatherColumn(columns.get(c), ts, ord));
        }
        return out;
    }

    static TsColumn gatherColumn(TsArray arr, long[] ts, int[] order) {
        if (arr instanceof TsArray.F64 a) {
            TsSeriesD s = new TsSeriesD();
            for (int i : order) {
                if (a.valid()[i]) {
                    s.push(ts[i], a.values()[i]);
                }
            }
            return new TsColumn.F64(s);
        } else if (arr instanceof TsArray.I64 a) {
            TsSeriesL s = new TsSeriesL();
            for (int i : order) {
                if (a.valid()[i]) {
                    s.push(ts[i], a.values()[i]);
                }
            }
            return new TsColumn.I64(s);
        } else if (arr instanceof TsArray.Bool a) {
            TsSeries<Boolean> s = new TsSeries<>();
            for (int i : order) {
                if (a.valid()[i]) {
                    s.push(ts[i], a.values()[i]);
                }
            }
            return new TsColumn.Bool(s);
        } else {
            TsArray.Str a = (TsArray.Str) arr;
            TsSeries<String> s = new TsSeries<>();
            for (int i : order) {
                if (a.valid()[i]) {
                    s.push(ts[i], a.values()[i]);
                }
            }
            return new TsColumn.Str(s);
        }
    }

    @Override
    public boolean equals(Object o) {
        if (!(o instanceof ResultFrame other)) {
            return false;
        }
        if (!java.util.Arrays.equals(ts, other.ts)) {
            return false;
        }
        if (!names.equals(other.names)) {
            return false;
        }
        return columns.equals(other.columns);
    }

    @Override
    public int hashCode() {
        int h = java.util.Arrays.hashCode(ts);
        h = 31 * h + names.hashCode();
        h = 31 * h + columns.hashCode();
        return h;
    }

    // Build a ResultFrame from in-flight state (used by the executor).
    static ResultFrame of(long[] ts, List<String> names, List<TsArray> columns) {
        return new ResultFrame(ts, new ArrayList<>(names), new ArrayList<>(columns));
    }
}
