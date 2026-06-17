package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Optional;

/**
 * Multi-way merge over a panel's slots, yielding aligned rows in ts order.
 * Each row is {@code (ts, values)} where {@code values.get(i)} is present if
 * slot {@code i} had a point at that ts, else empty.
 */
public final class TsPanelAligned<T> implements Iterable<TsPanelAligned.Row<T>> {

    /** One aligned row: a timestamp plus a per-slot column of optional values. */
    public record Row<T>(long ts, List<Optional<T>> values) {}

    private final List<List<TsPoint<T>>> columns;

    TsPanelAligned(List<List<TsPoint<T>>> columns) {
        this.columns = columns;
    }

    @Override
    public Iterator<Row<T>> iterator() {
        return new Iterator<>() {
            private final int[] cursor = new int[columns.size()];

            @Override
            public boolean hasNext() {
                for (int i = 0; i < columns.size(); i++) {
                    if (cursor[i] < columns.get(i).size()) return true;
                }
                return false;
            }

            @Override
            public Row<T> next() {
                long minTs = Long.MAX_VALUE;
                boolean any = false;
                for (int i = 0; i < columns.size(); i++) {
                    if (cursor[i] < columns.get(i).size()) {
                        long ts = columns.get(i).get(cursor[i]).ts();
                        if (!any || ts < minTs) minTs = ts;
                        any = true;
                    }
                }
                if (!any) throw new NoSuchElementException();
                List<Optional<T>> row = new ArrayList<>(columns.size());
                for (int i = 0; i < columns.size(); i++) {
                    List<TsPoint<T>> col = columns.get(i);
                    if (cursor[i] < col.size() && col.get(cursor[i]).ts() == minTs) {
                        row.add(Optional.ofNullable(col.get(cursor[i]).value()));
                        cursor[i]++;
                    } else {
                        row.add(Optional.empty());
                    }
                }
                return new Row<>(minTs, row);
            }
        };
    }

    public List<Row<T>> toList() {
        List<Row<T>> out = new ArrayList<>();
        for (Row<T> r : this) out.add(r);
        return out;
    }
}
