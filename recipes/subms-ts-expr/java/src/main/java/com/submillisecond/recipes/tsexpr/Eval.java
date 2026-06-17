package com.submillisecond.recipes.tsexpr;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;
import com.submillisecond.recipes.ts.TsValue;

/**
 * The evaluator. {@link #eval} walks a {@link TsExpr} tree against a
 * {@link TsDataFrame}, producing one typed {@link TsArray} aligned to the
 * frame's union-of-timestamps row axis. Evaluation is column-at-a-time: each
 * node materialises its whole array, then the parent combines child arrays
 * elementwise. The analytical-front model (throughput), not the per-op
 * tick-loop model.
 *
 * <p>The type rules live here, not in the IR: a {@code Col} takes its column's
 * dtype, arithmetic promotes I64/F64 to F64, comparison yields Bool, an Agg
 * resolves the result type its reduction defines. A mismatch the rules cannot
 * reconcile is a {@link TsExprException.Kind#TYPE_MISMATCH}.
 */
public final class Eval {

    private Eval() {}

    private static final class RowAxis {
        final int nrows;
        final Map<String, TsDataType> types;
        final Map<String, TsArray> columns;

        RowAxis(int nrows, Map<String, TsDataType> types, Map<String, TsArray> columns) {
            this.nrows = nrows;
            this.types = types;
            this.columns = columns;
        }

        static RowAxis build(TsDataFrame frame) {
            List<String> order = frame.columnNames();
            int nslots = order.size();

            Map<String, TsDataType> types = new HashMap<>(nslots * 2);
            for (String name : order) {
                frame.column(name).ifPresent(c -> types.put(name, c.dataType()));
            }

            List<TsDataFrame.Row> rows = frame.aligned();
            int nrows = rows.size();

            // Per-column dense cell lists over the row axis, projected to typed
            // arrays once so a deep tree pays the aligned-walk cost a single time.
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

            Map<String, TsArray> columns = new HashMap<>(nslots * 2);
            for (int s = 0; s < nslots; s++) {
                String name = order.get(s);
                TsDataType ty = types.getOrDefault(name, TsDataType.F64);
                columns.put(name, columnToArray(ty, cells.get(s)));
            }
            return new RowAxis(nrows, types, columns);
        }

        TsArray column(String name) {
            return columns.get(name);
        }
    }

    private static TsArray columnToArray(TsDataType ty, List<Optional<TsValue>> cells) {
        int n = cells.size();
        switch (ty) {
            case F64, VALUE -> {
                double[] values = new double[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    OptionalDoubleCell c = asF64(cells.get(i));
                    if (c.present) {
                        values[i] = c.value;
                        valid[i] = true;
                    }
                }
                return new TsArray.F64(values, valid);
            }
            case I64 -> {
                long[] values = new long[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.I64 v) {
                        values[i] = v.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.I64(values, valid);
            }
            case BOOL -> {
                boolean[] values = new boolean[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.Bool v) {
                        values[i] = v.value();
                        valid[i] = true;
                    }
                }
                return new TsArray.Bool(values, valid);
            }
            case STR -> {
                String[] values = new String[n];
                boolean[] valid = new boolean[n];
                for (int i = 0; i < n; i++) {
                    Optional<TsValue> c = cells.get(i);
                    if (c.isPresent() && c.get() instanceof TsValue.Str v) {
                        values[i] = v.value();
                        valid[i] = true;
                    } else {
                        values[i] = "";
                    }
                }
                return new TsArray.Str(values, valid);
            }
            default -> throw new IllegalStateException("unreachable dtype " + ty);
        }
    }

    private record OptionalDoubleCell(boolean present, double value) {}

    private static OptionalDoubleCell asF64(Optional<TsValue> c) {
        if (c.isPresent()) {
            TsValue v = c.get();
            if (v instanceof TsValue.F64 d) return new OptionalDoubleCell(true, d.value());
            if (v instanceof TsValue.I64 l) return new OptionalDoubleCell(true, l.value());
        }
        return new OptionalDoubleCell(false, 0.0);
    }

    /**
     * Evaluate {@code expr} over {@code frame}, returning a typed
     * {@link TsArray} aligned to the frame's union-of-timestamps row axis.
     */
    public static TsArray eval(TsExpr expr, TsDataFrame frame) {
        RowAxis axis = RowAxis.build(frame);
        return evalNode(expr, axis);
    }

    /**
     * Convenience for a top-level {@link TsExpr.Agg}: evaluate and return the
     * single scalar {@link TsValue}. The operators' per-group / per-partition
     * entry point. Throws {@link TsExprException#notScalar()} for any other
     * top-level shape. An empty frame yields the reduction's empty value
     * (Count -> 0, Sum -> 0.0, Mean -> NaN, Min/Max -> Null).
     */
    public static TsValue evalScalar(TsExpr expr, TsDataFrame frame) {
        if (!(expr instanceof TsExpr.Agg agg)) {
            throw TsExprException.notScalar();
        }
        RowAxis axis = RowAxis.build(frame);
        TsArray arr = evalNode(expr, axis);
        if (arr.isEmpty()) {
            return emptyAggScalar(agg.op());
        }
        return arr.get(0).orElse(TsValue.nullValue());
    }

    private static TsArray evalNode(TsExpr expr, RowAxis axis) {
        if (expr instanceof TsExpr.Col col) {
            TsArray c = axis.column(col.name());
            if (c == null) {
                throw TsExprException.unknownColumn(col.name());
            }
            return c;
        } else if (expr instanceof TsExpr.Lit lit) {
            return broadcastLit(lit.value(), axis.nrows);
        } else if (expr instanceof TsExpr.Unary u) {
            return evalUnary(u.op(), evalNode(u.operand(), axis));
        } else if (expr instanceof TsExpr.Binary b) {
            return evalBinary(b.op(), evalNode(b.lhs(), axis), evalNode(b.rhs(), axis));
        } else if (expr instanceof TsExpr.Compare cmp) {
            return evalCompare(cmp.op(), evalNode(cmp.lhs(), axis), evalNode(cmp.rhs(), axis));
        } else if (expr instanceof TsExpr.When w) {
            return evalWhen(
                    evalNode(w.cond(), axis),
                    evalNode(w.then(), axis),
                    evalNode(w.otherwise(), axis));
        } else if (expr instanceof TsExpr.Agg agg) {
            TsArray c = evalNode(agg.operand(), axis);
            return broadcastScalar(reduce(agg.op(), c), axis.nrows);
        }
        throw new IllegalStateException("unreachable expr variant");
    }

    private static TsArray broadcastLit(TsValue v, int n) {
        return broadcastScalar(v, n);
    }

    private static TsArray broadcastScalar(TsValue v, int n) {
        if (v instanceof TsValue.F64 x) {
            double[] vs = new double[n];
            java.util.Arrays.fill(vs, x.value());
            return new TsArray.F64(vs, TsArray.allTrue(n));
        } else if (v instanceof TsValue.I64 x) {
            long[] vs = new long[n];
            java.util.Arrays.fill(vs, x.value());
            return new TsArray.I64(vs, TsArray.allTrue(n));
        } else if (v instanceof TsValue.Bool x) {
            boolean[] vs = new boolean[n];
            java.util.Arrays.fill(vs, x.value());
            return new TsArray.Bool(vs, TsArray.allTrue(n));
        } else if (v instanceof TsValue.Str x) {
            String[] vs = new String[n];
            java.util.Arrays.fill(vs, x.value());
            return new TsArray.Str(vs, TsArray.allTrue(n));
        }
        // A null scalar (Min/Max over an all-null operand) broadcasts all-null.
        return new TsArray.F64(new double[n], new boolean[n]);
    }

    private static TsArray evalUnary(TsUnaryOp op, TsArray c) {
        if (c instanceof TsArray.F64 a) {
            double[] out = new double[a.len()];
            for (int i = 0; i < a.len(); i++) {
                out[i] = op == TsUnaryOp.NEG ? -a.values()[i] : Math.abs(a.values()[i]);
            }
            return new TsArray.F64(out, a.valid().clone());
        } else if (c instanceof TsArray.I64 a) {
            long[] out = new long[a.len()];
            for (int i = 0; i < a.len(); i++) {
                out[i] = op == TsUnaryOp.NEG ? -a.values()[i] : Math.abs(a.values()[i]);
            }
            return new TsArray.I64(out, a.valid().clone());
        }
        throw TsExprException.typeMismatch(
                "unary " + op + " requires a numeric operand, got " + c.dataType());
    }

    private static boolean isNumeric(TsArray a) {
        return a instanceof TsArray.F64 || a instanceof TsArray.I64;
    }

    private static double[] toF64(TsArray a) {
        if (a instanceof TsArray.F64 d) return d.values().clone();
        if (a instanceof TsArray.I64 l) {
            double[] out = new double[l.len()];
            for (int i = 0; i < l.len(); i++) out[i] = l.values()[i];
            return out;
        }
        throw new IllegalStateException("toF64 on non-numeric array");
    }

    private static TsArray evalBinary(TsBinaryOp op, TsArray l, TsArray r) {
        if (l instanceof TsArray.I64 li && r instanceof TsArray.I64 ri) {
            int n = li.len();
            long[] values = new long[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                if (!(li.valid()[i] && ri.valid()[i])) continue;
                long a = li.values()[i];
                long b = ri.values()[i];
                switch (op) {
                    case ADD -> { values[i] = a + b; valid[i] = true; }
                    case SUB -> { values[i] = a - b; valid[i] = true; }
                    case MUL -> { values[i] = a * b; valid[i] = true; }
                    case DIV -> {
                        if (b == 0L) continue; // zero divisor -> missing cell
                        values[i] = a / b;
                        valid[i] = true;
                    }
                }
            }
            return new TsArray.I64(values, valid);
        }
        if (isNumeric(l) && isNumeric(r)) {
            double[] lv = toF64(l);
            double[] rv = toF64(r);
            boolean[] lok = l.valid();
            boolean[] rok = r.valid();
            int n = lv.length;
            double[] values = new double[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                if (!(lok[i] && rok[i])) continue;
                double a = lv[i];
                double b = rv[i];
                switch (op) {
                    case ADD -> { values[i] = a + b; valid[i] = true; }
                    case SUB -> { values[i] = a - b; valid[i] = true; }
                    case MUL -> { values[i] = a * b; valid[i] = true; }
                    case DIV -> {
                        if (b == 0.0) continue; // divide-by-zero -> missing cell
                        values[i] = a / b;
                        valid[i] = true;
                    }
                }
            }
            return new TsArray.F64(values, valid);
        }
        throw TsExprException.typeMismatch("arithmetic " + op
                + " requires numeric operands, got " + l.dataType() + " and " + r.dataType());
    }

    private static TsArray evalCompare(TsCmpOp op, TsArray l, TsArray r) {
        int n = l.len();
        boolean[] values = new boolean[n];
        boolean[] valid = new boolean[n];
        if (isNumeric(l) && isNumeric(r)) {
            double[] lv = toF64(l);
            double[] rv = toF64(r);
            boolean[] lok = l.valid();
            boolean[] rok = r.valid();
            for (int i = 0; i < n; i++) {
                if (lok[i] && rok[i]) {
                    values[i] = cmpDouble(op, lv[i], rv[i]);
                    valid[i] = true;
                }
            }
        } else if (l instanceof TsArray.Str ls && r instanceof TsArray.Str rs) {
            for (int i = 0; i < n; i++) {
                if (ls.valid()[i] && rs.valid()[i]) {
                    values[i] = cmpStr(op, ls.values()[i], rs.values()[i]);
                    valid[i] = true;
                }
            }
        } else if (l instanceof TsArray.Bool lb && r instanceof TsArray.Bool rb) {
            for (int i = 0; i < n; i++) {
                if (!(lb.valid()[i] && rb.valid()[i])) continue;
                boolean res = switch (op) {
                    case EQ -> lb.values()[i] == rb.values()[i];
                    case NE -> lb.values()[i] != rb.values()[i];
                    default -> throw TsExprException.typeMismatch(
                            "bool compare supports only eq / ne");
                };
                values[i] = res;
                valid[i] = true;
            }
        } else {
            throw TsExprException.typeMismatch("compare " + op
                    + " requires same-type operands, got " + l.dataType() + " and " + r.dataType());
        }
        return new TsArray.Bool(values, valid);
    }

    private static boolean cmpDouble(TsCmpOp op, double a, double b) {
        return switch (op) {
            case LT -> a < b;
            case LE -> a <= b;
            case EQ -> a == b;
            case NE -> a != b;
            case GE -> a >= b;
            case GT -> a > b;
        };
    }

    private static boolean cmpStr(TsCmpOp op, String a, String b) {
        int c = a.compareTo(b);
        return switch (op) {
            case LT -> c < 0;
            case LE -> c <= 0;
            case EQ -> c == 0;
            case NE -> c != 0;
            case GE -> c >= 0;
            case GT -> c > 0;
        };
    }

    private static TsArray evalWhen(TsArray cond, TsArray then, TsArray otherwise) {
        if (!(cond instanceof TsArray.Bool mask)) {
            throw TsExprException.typeMismatch(
                    "when condition must be Bool, got " + cond.dataType());
        }
        if (then.dataType() != otherwise.dataType()) {
            throw TsExprException.typeMismatch("when branches disagree: then is "
                    + then.dataType() + ", otherwise is " + otherwise.dataType());
        }
        int n = cond.len();
        boolean[] mv = mask.values();
        boolean[] mok = mask.valid();
        if (then instanceof TsArray.F64 t && otherwise instanceof TsArray.F64 f) {
            double[] values = new double[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                if (!mok[i]) continue;
                TsArray.F64 src = mv[i] ? t : f;
                if (src.valid()[i]) { values[i] = src.values()[i]; valid[i] = true; }
            }
            return new TsArray.F64(values, valid);
        } else if (then instanceof TsArray.I64 t && otherwise instanceof TsArray.I64 f) {
            long[] values = new long[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                if (!mok[i]) continue;
                TsArray.I64 src = mv[i] ? t : f;
                if (src.valid()[i]) { values[i] = src.values()[i]; valid[i] = true; }
            }
            return new TsArray.I64(values, valid);
        } else if (then instanceof TsArray.Bool t && otherwise instanceof TsArray.Bool f) {
            boolean[] values = new boolean[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                if (!mok[i]) continue;
                TsArray.Bool src = mv[i] ? t : f;
                if (src.valid()[i]) { values[i] = src.values()[i]; valid[i] = true; }
            }
            return new TsArray.Bool(values, valid);
        } else {
            TsArray.Str t = (TsArray.Str) then;
            TsArray.Str f = (TsArray.Str) otherwise;
            String[] values = new String[n];
            boolean[] valid = new boolean[n];
            for (int i = 0; i < n; i++) {
                values[i] = "";
                if (!mok[i]) continue;
                TsArray.Str src = mv[i] ? t : f;
                if (src.valid()[i]) { values[i] = src.values()[i]; valid[i] = true; }
            }
            return new TsArray.Str(values, valid);
        }
    }

    private static TsValue reduce(TsAggOp op, TsArray c) {
        if (op == TsAggOp.COUNT) {
            return TsValue.ofLong(c.validCount());
        }
        return switch (op) {
            case SUM, MEAN -> {
                double[] vals = numericValid(c);
                double s = 0.0;
                for (double v : vals) s += v;
                if (op == TsAggOp.SUM) {
                    yield TsValue.ofDouble(s);
                }
                yield TsValue.ofDouble(vals.length == 0 ? Double.NaN : s / vals.length);
            }
            case MIN, MAX -> minMax(op, c);
            case COUNT -> throw new IllegalStateException("unreachable");
        };
    }

    private static double[] numericValid(TsArray c) {
        if (c instanceof TsArray.F64 a) {
            double[] out = new double[a.validCount()];
            int j = 0;
            for (int i = 0; i < a.len(); i++) if (a.valid()[i]) out[j++] = a.values()[i];
            return out;
        } else if (c instanceof TsArray.I64 a) {
            double[] out = new double[a.validCount()];
            int j = 0;
            for (int i = 0; i < a.len(); i++) if (a.valid()[i]) out[j++] = a.values()[i];
            return out;
        }
        throw TsExprException.typeMismatch(
                "sum / mean require a numeric operand, got " + c.dataType());
    }

    private static TsValue minMax(TsAggOp op, TsArray c) {
        boolean pickFirst = op == TsAggOp.MIN;
        if (c instanceof TsArray.F64 a) {
            Double acc = null;
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i] && (acc == null || keep(pickFirst, a.values()[i] < acc))) {
                    acc = a.values()[i];
                }
            }
            return acc == null ? TsValue.nullValue() : TsValue.ofDouble(acc);
        } else if (c instanceof TsArray.I64 a) {
            Long acc = null;
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i] && (acc == null || keep(pickFirst, a.values()[i] < acc))) {
                    acc = a.values()[i];
                }
            }
            return acc == null ? TsValue.nullValue() : TsValue.ofLong(acc);
        } else if (c instanceof TsArray.Str a) {
            String acc = null;
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i] && (acc == null
                        || keep(pickFirst, a.values()[i].compareTo(acc) < 0))) {
                    acc = a.values()[i];
                }
            }
            return acc == null ? TsValue.nullValue() : TsValue.ofString(acc);
        }
        throw TsExprException.typeMismatch("min / max are not defined over " + c.dataType());
    }

    private static boolean keep(boolean pickFirst, boolean less) {
        return pickFirst == less;
    }

    private static TsValue emptyAggScalar(TsAggOp op) {
        return switch (op) {
            case COUNT -> TsValue.ofLong(0L);
            case SUM -> TsValue.ofDouble(0.0);
            case MEAN -> TsValue.ofDouble(Double.NaN);
            case MIN, MAX -> TsValue.nullValue();
        };
    }
}
