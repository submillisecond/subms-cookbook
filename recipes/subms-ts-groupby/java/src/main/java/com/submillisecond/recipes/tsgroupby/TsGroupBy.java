package com.submillisecond.recipes.tsgroupby;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.Eval;
import com.submillisecond.recipes.tsexpr.TsArray;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsexpr.TsExprException;

/**
 * A built partition of a frame's rows by a tuple of typed key columns, ready to
 * be aggregated. Construct with {@link GroupBy#groupBy}.
 */
public final class TsGroupBy {

    /** An {@code (out_name, expr)} aggregation for {@link #agg}. */
    public record Agg(String name, TsExpr expr) {}

    private final List<String> keys;
    private final List<GroupBy.GroupSlot> groups;
    private final GroupBy.RowAxis axis;

    TsGroupBy(List<String> keys, List<GroupBy.GroupSlot> groups, GroupBy.RowAxis axis) {
        this.keys = keys;
        this.groups = groups;
        this.axis = axis;
    }

    /** Number of groups (one per distinct key tuple). */
    public int ngroups() {
        return groups.size();
    }

    /** The typed key tuple of group {@code g}, in key-column order. */
    public List<TsValue> key(int g) {
        List<KeyCell> k = groups.get(g).key;
        List<TsValue> out = new ArrayList<>(k.size());
        for (KeyCell c : k) {
            out.add(keyToValue(c));
        }
        return out;
    }

    /** Number of rows that fell into group {@code g}. */
    public int groupSize(int g) {
        return groups.get(g).rows.size();
    }

    List<GroupBy.GroupSlot> groups() {
        return groups;
    }

    List<String> keyNames() {
        return keys;
    }

    /**
     * Reduce each {@code (name, expr)} aggregation per group, returning a table
     * with one row per group: the key columns followed by the aggregated
     * columns. Each {@code expr} must be a top-level {@link TsExpr.Agg} (e.g.
     * {@code TsExpr.col("x").sum()}); a non-agg expr is rejected because it
     * would be per-row, not a single value per group.
     */
    public TsGroupResult agg(Agg... aggs) {
        for (Agg a : aggs) {
            if (!(a.expr() instanceof TsExpr.Agg)) {
                throw GroupByException.notAnAggregation(a.name());
            }
        }

        int ngroups = groups.size();
        List<TsGroupResult.Column> cols = new ArrayList<>(keys.size() + aggs.length);

        // Key columns, each rebuilt as a typed fully-valid array from the keys.
        for (int ki = 0; ki < keys.size(); ki++) {
            List<KeyCell> cells = new ArrayList<>(ngroups);
            for (GroupBy.GroupSlot s : groups) {
                cells.add(s.key.get(ki));
            }
            cols.add(new TsGroupResult.Column(keys.get(ki), GroupBy.keyArray(cells)));
        }

        // Aggregation columns: per group, build the sub-frame and eval each expr.
        List<List<TsValue>> scalars = new ArrayList<>(aggs.length);
        for (int ai = 0; ai < aggs.length; ai++) {
            scalars.add(new ArrayList<>(ngroups));
        }
        for (int g = 0; g < ngroups; g++) {
            TsDataFrame frame = groupFrame(g);
            for (int ai = 0; ai < aggs.length; ai++) {
                TsValue scalar;
                try {
                    scalar = Eval.evalScalar(aggs[ai].expr(), frame);
                } catch (TsExprException e) {
                    throw GroupByException.unknownColumn(describeExprCol(aggs[ai].expr()));
                }
                scalars.get(ai).add(scalar);
            }
        }
        for (int ai = 0; ai < aggs.length; ai++) {
            cols.add(new TsGroupResult.Column(aggs[ai].name(), scalarArray(scalars.get(ai))));
        }

        return new TsGroupResult(cols, ngroups);
    }

    // Build a frame holding only the rows of group g, at synthetic monotonic
    // timestamps so the evaluator's row axis lines up 1:1 with the group's rows.
    private TsDataFrame groupFrame(int g) {
        List<Integer> rows = groups.get(g).rows;
        TsDataFrame f = new TsDataFrame();
        for (int ci = 0; ci < axis.names.size(); ci++) {
            f.pushColumn(axis.names.get(ci), GroupBy.buildColumn(axis.cells.get(ci), rows));
        }
        return f;
    }

    private static TsValue keyToValue(KeyCell c) {
        if (c instanceof KeyCell.LongKey k) {
            return TsValue.ofLong(k.value());
        }
        if (c instanceof KeyCell.DoubleKey k) {
            return TsValue.ofDouble(k.value());
        }
        if (c instanceof KeyCell.BoolKey k) {
            return TsValue.ofBool(k.value());
        }
        return TsValue.ofString(((KeyCell.StrKey) c).value());
    }

    // Build a TsArray from per-group scalar reduction results. The aggregation
    // type is uniform across groups, so the first non-null scalar picks the
    // variant; a Null / NaN cell becomes an invalid slot (a Mean over an empty /
    // all-null group is NaN, surfaced as a null, never a raw NaN downstream).
    private static TsArray scalarArray(List<TsValue> scalars) {
        GroupBy.ValueKind kind = GroupBy.ValueKind.F64;
        for (TsValue v : scalars) {
            if (v instanceof TsValue.F64 d && Double.isNaN(d.value())) {
                continue;
            }
            if (v instanceof TsValue.Null) {
                continue;
            }
            kind = GroupBy.ValueKind.of(v);
            break;
        }

        int n = scalars.size();
        switch (kind) {
            case I64 -> {
                long[] values = new long[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    if (scalars.get(i) instanceof TsValue.I64 x) {
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
                    if (scalars.get(i) instanceof TsValue.Bool x) {
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
                    if (scalars.get(i) instanceof TsValue.Str x) {
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
                    TsValue v = scalars.get(i);
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

    // Best-effort column name for an unknown-column error raised inside
    // evalScalar: the first Col leaf in the expression tree.
    private static String describeExprCol(TsExpr expr) {
        return firstCol(expr).orElse("<unknown>");
    }

    private static Optional<String> firstCol(TsExpr e) {
        if (e instanceof TsExpr.Col c) {
            return Optional.of(c.name());
        } else if (e instanceof TsExpr.Lit) {
            return Optional.empty();
        } else if (e instanceof TsExpr.Unary u) {
            return firstCol(u.operand());
        } else if (e instanceof TsExpr.Agg a) {
            return firstCol(a.operand());
        } else if (e instanceof TsExpr.Binary b) {
            return firstCol(b.lhs()).or(() -> firstCol(b.rhs()));
        } else if (e instanceof TsExpr.Compare cmp) {
            return firstCol(cmp.lhs()).or(() -> firstCol(cmp.rhs()));
        } else if (e instanceof TsExpr.When w) {
            return firstCol(w.cond())
                    .or(() -> firstCol(w.then()))
                    .or(() -> firstCol(w.otherwise()));
        }
        return Optional.empty();
    }
}
