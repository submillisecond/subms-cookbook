package com.submillisecond.recipes.tscsv;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;

/**
 * Accumulates a column's surviving {@code (ts, raw)} cells during a read, then
 * infers the narrowest element type and materialises the typed
 * {@link TsColumn}. An empty cell never reaches here (gaps are dropped at
 * ingest), so an all-empty column is {@code Str} with zero points.
 *
 * <p>{@code forcedStr} pins the column to {@code Str} regardless of cell shape
 * - the NDJSON reader sets it when a value arrived quoted, so a quoted
 * {@code "1"} stays text rather than re-inferring to {@code I64}.
 */
final class TsColumnBuilder {

    private long[] ts = new long[8];
    private final List<String> raw = new ArrayList<>();
    private int n = 0;
    private boolean forcedStr = false;

    void add(long t, String cell) {
        if (n == ts.length) {
            long[] grown = new long[ts.length * 2];
            System.arraycopy(ts, 0, grown, 0, n);
            ts = grown;
        }
        ts[n++] = t;
        raw.add(cell);
    }

    void forceStr() {
        this.forcedStr = true;
    }

    TsColumn build() {
        TsInferredType kind = forcedStr ? TsInferredType.STR : TsTypeInfer.infer(raw);
        switch (kind) {
            case I64 -> {
                TsSeriesL s = TsSeriesL.withCapacity(n);
                for (int i = 0; i < n; i++) {
                    s.push(ts[i], Long.parseLong(raw.get(i)));
                }
                return new TsColumn.I64(s);
            }
            case F64 -> {
                TsSeriesD s = TsSeriesD.withCapacity(n);
                for (int i = 0; i < n; i++) {
                    double v = Double.parseDouble(raw.get(i));
                    if (Double.isFinite(v)) {
                        s.push(ts[i], v);
                    }
                }
                return new TsColumn.F64(s);
            }
            case BOOL -> {
                TsSeries<Boolean> s = TsSeries.withCapacity(n);
                for (int i = 0; i < n; i++) {
                    s.push(ts[i], raw.get(i).equalsIgnoreCase("true"));
                }
                return new TsColumn.Bool(s);
            }
            default -> {
                TsSeries<String> s = TsSeries.withCapacity(n);
                for (int i = 0; i < n; i++) {
                    s.push(ts[i], raw.get(i));
                }
                return new TsColumn.Str(s);
            }
        }
    }
}
