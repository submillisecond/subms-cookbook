package com.submillisecond.recipes.tslazy;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.Eval;
import com.submillisecond.recipes.tsexpr.TsArray;
import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * The executor. {@link #runPlan} walks an optimised {@link PlanNode} list and
 * produces a {@link ResultFrame}.
 *
 * <p>Execution materialises the source frame's union-of-timestamps row axis once
 * (via {@code subms-ts-expr} eval), then each node is a slice / gather / append
 * over that materialised state. Expr eval runs against an in-flight frame whose
 * row axis is the positional row index, plus an always-valid anchor column so a
 * row whose every real column is null at that position does not collapse out of
 * the aligned view and break positional alignment.
 *
 * <p>Behavioural parity with the Rust sibling's {@code exec.rs}.
 */
final class Executor {

    private Executor() {}

    // Reserved name for the eval anchor column; a user expr never names it.
    private static final String ANCHOR = "__subms_lazy_row";

    private static final class State {
        long[] ts;
        List<String> names;
        List<TsArray> columns;

        State(long[] ts, List<String> names, List<TsArray> columns) {
            this.ts = ts;
            this.names = names;
            this.columns = columns;
        }

        int nrows() {
            return ts.length;
        }

        int indexOf(String name) {
            return names.indexOf(name);
        }

        // A frame the evaluator runs against that preserves the in-flight ROW
        // ORDER exactly. Expr eval is positional / elementwise, so its output
        // array maps back 1:1 to the in-flight rows by position. Row index is
        // the synthetic monotonic ts (a real axis may be non-monotonic after a
        // sort, which TsSeries.push rejects, and eval ignores the ts values).
        TsDataFrame frameForEval() {
            int n = nrows();
            TsDataFrame out = new TsDataFrame();
            TsSeriesL anchor = new TsSeriesL();
            for (long i = 0; i < n; i++) {
                anchor.push(i, i);
            }
            out.withColumn(ANCHOR, new TsColumn.I64(anchor));
            int[] order = new int[n];
            long[] idxTs = new long[n];
            for (int i = 0; i < n; i++) {
                order[i] = i;
                idxTs[i] = i;
            }
            for (int c = 0; c < names.size(); c++) {
                out.withColumn(names.get(c), ResultFrame.gatherColumn(columns.get(c), idxTs, order));
            }
            return out;
        }
    }

    static ResultFrame runPlan(TsDataFrame source, List<PlanNode> nodes) {
        State state = materialise(source);
        for (PlanNode node : nodes) {
            if (node instanceof PlanNode.Select s) {
                applySelect(state, s.columns());
            } else if (node instanceof PlanNode.Filter f) {
                applyFilter(state, f.predicate());
            } else if (node instanceof PlanNode.WithColumn w) {
                applyWithColumn(state, w.name(), w.expr());
            } else if (node instanceof PlanNode.SortBy sb) {
                applySort(state, sb.column(), sb.ascending());
            } else if (node instanceof PlanNode.Limit l) {
                applyLimit(state, l.n());
            } else if (node instanceof PlanNode.Agg a) {
                return applyAgg(state, a.aggs());
            }
        }
        return ResultFrame.of(state.ts, state.names, state.columns);
    }

    private static State materialise(TsDataFrame frame) {
        List<String> names = new ArrayList<>(frame.columnNames());
        List<TsDataFrame.Row> rows = frame.aligned();
        long[] ts = new long[rows.size()];
        for (int i = 0; i < rows.size(); i++) {
            ts[i] = rows.get(i).ts();
        }
        List<TsArray> columns = new ArrayList<>(names.size());
        for (String name : names) {
            columns.add(Eval.eval(TsExpr.col(name), frame));
        }
        return new State(ts, names, columns);
    }

    private static void applySelect(State state, List<String> cols) {
        List<String> names = new ArrayList<>(cols.size());
        List<TsArray> columns = new ArrayList<>(cols.size());
        for (String c : cols) {
            int i = state.indexOf(c);
            if (i >= 0) {
                names.add(state.names.get(i));
                columns.add(state.columns.get(i));
            }
        }
        state.names = names;
        state.columns = columns;
    }

    private static void applyFilter(State state, TsExpr pred) {
        TsDataFrame frame = state.frameForEval();
        TsArray mask = Eval.eval(pred, frame);
        if (!(mask instanceof TsArray.Bool b)) {
            throw LazyException.nonBoolPredicate(mask.dataType());
        }
        boolean[] values = b.values();
        boolean[] valid = b.valid();
        int n = state.nrows();
        boolean[] keep = new boolean[n];
        for (int i = 0; i < n; i++) {
            keep[i] = i < valid.length && valid[i] && values[i];
        }
        gatherRows(state, keep);
    }

    private static void gatherRows(State state, boolean[] keep) {
        int n = state.nrows();
        int kept = 0;
        for (boolean k : keep) {
            if (k) {
                kept++;
            }
        }
        int[] idx = new int[kept];
        int j = 0;
        for (int i = 0; i < n; i++) {
            if (keep[i]) {
                idx[j++] = i;
            }
        }
        permute(state, idx);
    }

    private static void applyWithColumn(State state, String name, TsExpr expr) {
        TsDataFrame frame = state.frameForEval();
        TsArray arr = Eval.eval(expr, frame);
        int i = state.indexOf(name);
        if (i >= 0) {
            state.columns.set(i, arr);
        } else {
            state.names.add(name);
            state.columns.add(arr);
        }
    }

    private static void applySort(State state, String column, boolean ascending) {
        int i = state.indexOf(column);
        if (i < 0) {
            throw LazyException.unknownSortColumn(column);
        }
        TsArray key = state.columns.get(i);
        int n = state.nrows();
        Integer[] order = new Integer[n];
        for (int r = 0; r < n; r++) {
            order[r] = r;
        }
        // Stable sort, nulls last in both directions.
        java.util.Arrays.sort(order, (a, b) -> cmpCells(key, a, b, ascending));
        int[] ord = new int[n];
        for (int r = 0; r < n; r++) {
            ord[r] = order[r];
        }
        permute(state, ord);
    }

    private static void applyLimit(State state, int n) {
        int keep = Math.min(n, state.nrows());
        int[] idx = new int[keep];
        for (int i = 0; i < keep; i++) {
            idx[i] = i;
        }
        permute(state, idx);
    }

    private static ResultFrame applyAgg(State state, List<PlanNode.NamedExpr> aggs) {
        TsDataFrame frame = state.frameForEval();
        List<String> names = new ArrayList<>(aggs.size());
        List<TsArray> columns = new ArrayList<>(aggs.size());
        for (PlanNode.NamedExpr ne : aggs) {
            TsValue scalar = Eval.evalScalar(ne.expr(), frame);
            names.add(ne.name());
            columns.add(scalarToArray(scalar));
        }
        return ResultFrame.of(new long[] {0L}, names, columns);
    }

    private static void permute(State state, int[] idx) {
        long[] ts = new long[idx.length];
        for (int i = 0; i < idx.length; i++) {
            ts[i] = state.ts[idx[i]];
        }
        state.ts = ts;
        for (int c = 0; c < state.columns.size(); c++) {
            state.columns.set(c, takeRows(state.columns.get(c), idx));
        }
    }

    private static int cmpCells(TsArray arr, int a, int b, boolean ascending) {
        boolean va = a < arr.valid().length && arr.valid()[a];
        boolean vb = b < arr.valid().length && arr.valid()[b];
        if (!va && !vb) {
            return 0;
        }
        if (!va) {
            return 1; // nulls last
        }
        if (!vb) {
            return -1;
        }
        int ord = cmpPresent(arr, a, b);
        return ascending ? ord : -ord;
    }

    private static int cmpPresent(TsArray arr, int a, int b) {
        if (arr instanceof TsArray.F64 x) {
            return Double.compare(x.values()[a], x.values()[b]);
        } else if (arr instanceof TsArray.I64 x) {
            return Long.compare(x.values()[a], x.values()[b]);
        } else if (arr instanceof TsArray.Bool x) {
            return Boolean.compare(x.values()[a], x.values()[b]);
        } else {
            TsArray.Str x = (TsArray.Str) arr;
            return x.values()[a].compareTo(x.values()[b]);
        }
    }

    private static TsArray takeRows(TsArray arr, int[] idx) {
        int n = idx.length;
        if (arr instanceof TsArray.F64 a) {
            double[] values = new double[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                values[i] = a.values()[idx[i]];
                valid[i] = a.valid()[idx[i]];
            }
            return new TsArray.F64(values, valid);
        } else if (arr instanceof TsArray.I64 a) {
            long[] values = new long[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                values[i] = a.values()[idx[i]];
                valid[i] = a.valid()[idx[i]];
            }
            return new TsArray.I64(values, valid);
        } else if (arr instanceof TsArray.Bool a) {
            boolean[] values = new boolean[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                values[i] = a.values()[idx[i]];
                valid[i] = a.valid()[idx[i]];
            }
            return new TsArray.Bool(values, valid);
        } else {
            TsArray.Str a = (TsArray.Str) arr;
            String[] values = new String[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                values[i] = a.values()[idx[i]];
                valid[i] = a.valid()[idx[i]];
            }
            return new TsArray.Str(values, valid);
        }
    }

    private static TsArray scalarToArray(TsValue v) {
        if (v instanceof TsValue.F64 x) {
            return new TsArray.F64(new double[] {x.value()}, new boolean[] {true});
        } else if (v instanceof TsValue.I64 x) {
            return new TsArray.I64(new long[] {x.value()}, new boolean[] {true});
        } else if (v instanceof TsValue.Bool x) {
            return new TsArray.Bool(new boolean[] {x.value()}, new boolean[] {true});
        } else if (v instanceof TsValue.Str x) {
            return new TsArray.Str(new String[] {x.value()}, new boolean[] {true});
        }
        // A null reduction (Min/Max over all-null) is a single null F64 cell.
        return new TsArray.F64(new double[] {0.0}, new boolean[] {false});
    }
}
