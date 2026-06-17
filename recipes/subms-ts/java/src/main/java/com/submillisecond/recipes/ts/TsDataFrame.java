package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Optional;

/**
 * A heterogeneous bag of named, typed columns - the analytical foundation the
 * rest of the arc builds on. A {@link TsPanel} is homogeneous (every column
 * shares one type); a frame is per-column typed via {@link TsColumn}. Column
 * insertion order is the frame's column order.
 *
 * <p>The genericity lives on the series, not the cell: each column holds a
 * homogeneous typed series, so columnar scans stay unboxed. Stored columns
 * never hold nulls; nulls are derived, surfacing only as empty cells in the
 * {@link #aligned} view where a column has a gap at a given ts.
 */
public final class TsDataFrame {

    private final List<String> names = new ArrayList<>();
    private final List<TsColumn> columns = new ArrayList<>();

    public TsDataFrame() {}

    /** Builder add. Throws {@link IllegalArgumentException} on a duplicate
     *  name; use {@link #pushColumn} for the fallible runtime path. */
    public TsDataFrame withColumn(String name, TsColumn col) {
        if (!pushColumn(name, col)) {
            throw new IllegalArgumentException("duplicate frame column: " + name);
        }
        return this;
    }

    /** Append a column. Returns {@code false} (no-op) on a duplicate name. */
    public boolean pushColumn(String name, TsColumn col) {
        if (indexOf(name) >= 0) return false;
        names.add(name);
        columns.add(col);
        return true;
    }

    private int indexOf(String name) {
        for (int i = 0; i < names.size(); i++) {
            if (names.get(i).equals(name)) return i;
        }
        return -1;
    }

    public Optional<TsColumn> column(String name) {
        int i = indexOf(name);
        return i < 0 ? Optional.empty() : Optional.of(columns.get(i));
    }

    public List<String> columnNames() {
        return List.copyOf(names);
    }

    /** The frame's (name, type) shape in column order, derived from the
     *  columns themselves. */
    public TsFrameSchema schema() {
        List<TsField> fields = new ArrayList<>(columns.size());
        for (int i = 0; i < columns.size(); i++) {
            fields.add(new TsField(names.get(i), columns.get(i).dataType()));
        }
        return new TsFrameSchema(fields);
    }

    public int ncols() {
        return columns.size();
    }

    public boolean isEmpty() {
        return columns.isEmpty();
    }

    /** Projection: a new frame referencing the named columns, in the requested
     *  order. Unknown names are skipped. */
    public TsDataFrame select(String... selected) {
        TsDataFrame out = new TsDataFrame();
        for (String name : selected) {
            int i = indexOf(name);
            if (i >= 0) {
                out.names.add(names.get(i));
                out.columns.add(columns.get(i));
            }
        }
        return out;
    }

    /** Remove and return a column by name. */
    public Optional<TsColumn> drop(String name) {
        int i = indexOf(name);
        if (i < 0) return Optional.empty();
        names.remove(i);
        return Optional.of(columns.remove(i));
    }

    /** Rename a column in place. Returns {@code false} if {@code from} is
     *  absent or {@code to} already names another column. */
    public boolean rename(String from, String to) {
        int i = indexOf(from);
        if (i < 0) return false;
        if (!from.equals(to) && indexOf(to) >= 0) return false;
        names.set(i, to);
        return true;
    }

    /**
     * Row-aligned view over the union of every column's ts axis. Each row is a
     * ts plus one {@code Optional<TsValue>} per column (in column order), empty
     * where that column has no point at that ts. This is the frame's null
     * surface; the gaps are derived here by the multi-way merge.
     */
    public List<Row> aligned() {
        return new AlignedView(columns).toList();
    }

    /** One aligned row: a timestamp plus a per-column cell of optional values. */
    public record Row(long ts, List<Optional<TsValue>> values) {}

    private static final class AlignedView {
        private final List<List<TsPoint<TsValue>>> cols;

        AlignedView(List<TsColumn> columns) {
            this.cols = new ArrayList<>(columns.size());
            for (TsColumn c : columns) {
                this.cols.add(boxColumn(c));
            }
        }

        Iterator<Row> iterator() {
            return new Iterator<>() {
                private final int[] cursor = new int[cols.size()];

                @Override
                public boolean hasNext() {
                    for (int i = 0; i < cols.size(); i++) {
                        if (cursor[i] < cols.get(i).size()) return true;
                    }
                    return false;
                }

                @Override
                public Row next() {
                    long minTs = Long.MAX_VALUE;
                    boolean any = false;
                    for (int i = 0; i < cols.size(); i++) {
                        if (cursor[i] < cols.get(i).size()) {
                            long ts = cols.get(i).get(cursor[i]).ts();
                            if (!any || ts < minTs) minTs = ts;
                            any = true;
                        }
                    }
                    if (!any) throw new NoSuchElementException();
                    List<Optional<TsValue>> row = new ArrayList<>(cols.size());
                    for (int i = 0; i < cols.size(); i++) {
                        List<TsPoint<TsValue>> col = cols.get(i);
                        if (cursor[i] < col.size() && col.get(cursor[i]).ts() == minTs) {
                            row.add(Optional.ofNullable(col.get(cursor[i]).value()));
                            cursor[i]++;
                        } else {
                            row.add(Optional.empty());
                        }
                    }
                    return new Row(minTs, row);
                }
            };
        }

        List<Row> toList() {
            List<Row> out = new ArrayList<>();
            Iterator<Row> it = iterator();
            while (it.hasNext()) out.add(it.next());
            return out;
        }
    }

    // Materialise a column as (ts, TsValue) points so the merge has one shape.
    private static List<TsPoint<TsValue>> boxColumn(TsColumn c) {
        List<TsPoint<TsValue>> out = new ArrayList<>(c.len());
        switch (c) {
            case TsColumn.F64 f -> {
                for (TsPoint<Double> p : f.series().toList()) {
                    out.add(new TsPoint<>(p.ts(), TsValue.ofDouble(p.value())));
                }
            }
            case TsColumn.I64 l -> {
                for (TsPoint<Long> p : l.series().toList()) {
                    out.add(new TsPoint<>(p.ts(), TsValue.ofLong(p.value())));
                }
            }
            case TsColumn.Bool b -> {
                for (TsPoint<Boolean> p : b.series()) {
                    out.add(new TsPoint<>(p.ts(), TsValue.ofBool(p.value())));
                }
            }
            case TsColumn.Str s -> {
                for (TsPoint<String> p : s.series()) {
                    out.add(new TsPoint<>(p.ts(), TsValue.ofString(p.value())));
                }
            }
            case TsColumn.Value v -> {
                for (TsPoint<TsValue> p : v.series()) {
                    out.add(new TsPoint<>(p.ts(), p.value()));
                }
            }
        }
        return out;
    }
}
