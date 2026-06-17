package com.submillisecond.recipes.tssql;

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
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsgroupby.GroupBy;
import com.submillisecond.recipes.tsgroupby.GroupByException;
import com.submillisecond.recipes.tsgroupby.TsGroupBy;
import com.submillisecond.recipes.tsgroupby.TsGroupResult;
import com.submillisecond.recipes.tslazy.LazyException;
import com.submillisecond.recipes.tslazy.LazyTsFrame;
import com.submillisecond.recipes.tslazy.ResultFrame;
import com.submillisecond.recipes.tssql.Ast.AggFunc;
import com.submillisecond.recipes.tssql.Ast.Aggregate;
import com.submillisecond.recipes.tssql.Ast.And;
import com.submillisecond.recipes.tssql.Ast.Arith;
import com.submillisecond.recipes.tssql.Ast.Case;
import com.submillisecond.recipes.tssql.Ast.CmpOp;
import com.submillisecond.recipes.tssql.Ast.Column;
import com.submillisecond.recipes.tssql.Ast.Compare;
import com.submillisecond.recipes.tssql.Ast.ExprItem;
import com.submillisecond.recipes.tssql.Ast.IntLit;
import com.submillisecond.recipes.tssql.Ast.Literal;
import com.submillisecond.recipes.tssql.Ast.Not;
import com.submillisecond.recipes.tssql.Ast.NumLit;
import com.submillisecond.recipes.tssql.Ast.Or;
import com.submillisecond.recipes.tssql.Ast.OrderKey;
import com.submillisecond.recipes.tssql.Ast.SelectItem;
import com.submillisecond.recipes.tssql.Ast.SqlExpr;
import com.submillisecond.recipes.tssql.Ast.SqlLiteral;
import com.submillisecond.recipes.tssql.Ast.Star;
import com.submillisecond.recipes.tssql.Ast.StrLit;
import com.submillisecond.recipes.tssql.Ast.TsSqlStmt;

/**
 * Lowering: turn a parsed {@link TsSqlStmt} into a result {@link TsDataFrame} by
 * compiling its clauses onto the already-built operator recipes. There is no
 * execution engine of its own here - every clause becomes a call into
 * {@code subms-ts-expr} (scalar / predicate IR), {@code subms-ts-lazy} (row-wise
 * pipeline), or {@code subms-ts-groupby} (the partition + per-group aggregate).
 * Mirrors the Rust {@code lower} module.
 */
final class Lower {

    private Lower() {}

    private static final String GROUP_ALL_KEY = "__subms_sql_all";

    static TsDataFrame execute(TsSqlStmt stmt, TsDataFrame source) {
        List<String> sourceCols = source.columnNames();
        validateColumns(stmt, sourceCols);

        boolean isAggregate = !stmt.groupBy().isEmpty() || projectionHasAggregate(stmt);
        return isAggregate
                ? executeAggregate(stmt, source, sourceCols)
                : executeRowwise(stmt, source, sourceCols);
    }

    private static boolean projectionHasAggregate(TsSqlStmt stmt) {
        for (SelectItem item : stmt.projection()) {
            if (item instanceof ExprItem e && e.expr().containsAggregate()) {
                return true;
            }
        }
        return false;
    }

    private static TsDataFrame cloneFrame(TsDataFrame source, List<String> cols) {
        return source.select(cols.toArray(new String[0]));
    }

    // ---------- row-wise pipeline ----------

    private static TsDataFrame executeRowwise(
            TsSqlStmt stmt, TsDataFrame source, List<String> sourceCols) {
        LazyTsFrame lazy = new LazyTsFrame(cloneFrame(source, sourceCols));

        if (stmt.filter() != null) {
            lazy = lazy.filter(lowerPredicate(stmt.filter()));
        }

        List<String> outNames = new ArrayList<>();
        for (SelectItem item : stmt.projection()) {
            if (item instanceof Star) {
                for (String c : sourceCols) {
                    if (!outNames.contains(c)) {
                        outNames.add(c);
                    }
                }
            } else {
                ExprItem e = (ExprItem) item;
                boolean bareColumn = e.expr() instanceof Column col && col.name().equals(e.alias());
                if (!bareColumn) {
                    lazy = lazy.withColumn(e.alias(), lowerScalar(e.expr()));
                }
                if (!outNames.contains(e.alias())) {
                    outNames.add(e.alias());
                }
            }
        }

        lazy = applyOrderBy(lazy, stmt.orderBy());

        lazy = lazy.select(outNames.toArray(new String[0]));

        if (stmt.limit().isPresent()) {
            lazy = lazy.limit(stmt.limit().get());
        }

        ResultFrame result;
        try {
            result = lazy.collect();
        } catch (RuntimeException e) {
            throw mapLazyError(e);
        }
        return resultToOrderedFrame(result);
    }

    // Apply a multi-key ORDER BY as a chain of stable single-column sorts. The
    // lazy sort is stable, so sorting by the LEAST-significant key first and the
    // most-significant key last yields the correct lexicographic multi-key order.
    private static LazyTsFrame applyOrderBy(LazyTsFrame lazy, List<OrderKey> keys) {
        for (int i = keys.size() - 1; i >= 0; i--) {
            OrderKey key = keys.get(i);
            lazy = lazy.sortBy(key.column(), key.ascending());
        }
        return lazy;
    }

    // Rebuild a TsDataFrame from a lazy ResultFrame using ROW POSITION as the
    // synthetic ts so the pipeline's row order (post ORDER BY) is preserved. The
    // lazy intoDataFrame re-sorts rows by their original ts, which would undo an
    // ORDER BY; a SQL result is positional, not temporal, so we re-key on the
    // row index.
    private static TsDataFrame resultToOrderedFrame(ResultFrame result) {
        int n = result.nrows();
        TsDataFrame frame = new TsDataFrame();
        for (String name : result.columnNames()) {
            TsArray arr = result.column(name).orElseThrow();
            frame.pushColumn(name, arrayToPositionalColumn(arr, n));
        }
        return frame;
    }

    private static TsColumn arrayToPositionalColumn(TsArray arr, int n) {
        if (arr instanceof TsArray.F64 a) {
            TsSeriesD s = new TsSeriesD();
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i]) {
                    s.push(i, a.values()[i]);
                }
            }
            return new TsColumn.F64(s);
        } else if (arr instanceof TsArray.I64 a) {
            TsSeriesL s = new TsSeriesL();
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i]) {
                    s.push(i, a.values()[i]);
                }
            }
            return new TsColumn.I64(s);
        } else if (arr instanceof TsArray.Bool a) {
            TsSeries<Boolean> s = new TsSeries<>();
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i]) {
                    s.push(i, a.values()[i]);
                }
            }
            return new TsColumn.Bool(s);
        } else {
            TsArray.Str a = (TsArray.Str) arr;
            TsSeries<String> s = new TsSeries<>();
            for (int i = 0; i < a.len(); i++) {
                if (a.valid()[i]) {
                    s.push(i, a.values()[i]);
                }
            }
            return new TsColumn.Str(s);
        }
    }

    // ---------- aggregate query ----------

    private static TsDataFrame executeAggregate(
            TsSqlStmt stmt, TsDataFrame source, List<String> sourceCols) {
        TsDataFrame owned = cloneFrame(source, sourceCols);

        List<TsGroupBy.Agg> aggSpecs = new ArrayList<>();
        for (SelectItem item : stmt.projection()) {
            if (item instanceof Star) {
                throw TsSqlException.type("SELECT * is not allowed with GROUP BY / aggregates");
            }
            ExprItem e = (ExprItem) item;
            if (e.expr().containsAggregate()) {
                aggSpecs.add(new TsGroupBy.Agg(e.alias(), lowerAggExpr(e.expr())));
            } else if (e.expr() instanceof Column col) {
                if (!stmt.groupBy().contains(col.name())) {
                    throw TsSqlException.type(
                            "column " + col.name() + " must appear in GROUP BY or an aggregate");
                }
                // A key column is emitted by groupBy directly; no agg spec.
            } else {
                throw TsSqlException.type(
                        "a non-aggregate projection must be a GROUP BY key column");
            }
        }

        boolean syntheticKey = stmt.groupBy().isEmpty();
        TsDataFrame frameWithKey;
        String[] keys;
        if (syntheticKey) {
            frameWithKey = withConstantKey(owned);
            keys = new String[] {GROUP_ALL_KEY};
        } else {
            frameWithKey = owned;
            keys = stmt.groupBy().toArray(new String[0]);
        }

        TsGroupResult grouped;
        try {
            grouped = GroupBy.groupBy(frameWithKey, keys)
                    .agg(aggSpecs.toArray(new TsGroupBy.Agg[0]));
        } catch (GroupByException e) {
            throw mapGroupByError(e);
        }

        TsDataFrame frame = groupResultToFrame(grouped);
        if (syntheticKey) {
            frame.drop(GROUP_ALL_KEY);
        }

        return applyPostAggregate(stmt, frame);
    }

    private static TsDataFrame withConstantKey(TsDataFrame frame) {
        TsDataFrame out = frame.select(frame.columnNames().toArray(new String[0]));
        TsSeriesL key = new TsSeriesL();
        for (TsDataFrame.Row row : frame.aligned()) {
            key.push(row.ts(), 0L);
        }
        out.pushColumn(GROUP_ALL_KEY, new TsColumn.I64(key));
        return out;
    }

    private static TsDataFrame applyPostAggregate(TsSqlStmt stmt, TsDataFrame frame) {
        if (stmt.orderBy().isEmpty() && stmt.limit().isEmpty()) {
            return frame;
        }
        // Capture the full column set so a terminal select pins it: the lazy
        // optimiser's projection pushdown drops any column no downstream node
        // references, and a lone sortBy references only its key.
        List<String> outNames = frame.columnNames();
        LazyTsFrame lazy = applyOrderBy(new LazyTsFrame(frame), stmt.orderBy());
        lazy = lazy.select(outNames.toArray(new String[0]));
        if (stmt.limit().isPresent()) {
            lazy = lazy.limit(stmt.limit().get());
        }
        ResultFrame result;
        try {
            result = lazy.collect();
        } catch (RuntimeException e) {
            throw mapLazyError(e);
        }
        return resultToOrderedFrame(result);
    }

    // A TsGroupResult is a set of named TsArrays. Rebuild a TsDataFrame by
    // gathering each array's present cells at synthetic monotonic timestamps so
    // the row axis lines up across columns (the aggregate collapsed the original
    // time axis).
    private static TsDataFrame groupResultToFrame(TsGroupResult result) {
        TsDataFrame frame = new TsDataFrame();
        for (String name : result.columnNames()) {
            TsArray arr = result.column(name).orElseThrow();
            frame.pushColumn(name, arrayToPositionalColumn(arr, arr.len()));
        }
        return frame;
    }

    // ---------- expression lowering ----------

    private static TsExpr lowerScalar(SqlExpr expr) {
        if (expr instanceof Column c) {
            return TsExpr.col(c.name());
        } else if (expr instanceof Literal l) {
            return lowerLiteral(l.value());
        } else if (expr instanceof Arith a) {
            TsExpr l = lowerScalar(a.lhs());
            TsExpr r = lowerScalar(a.rhs());
            return switch (a.op()) {
                case ADD -> l.add(r);
                case SUB -> l.sub(r);
                case MUL -> l.mul(r);
                case DIV -> l.div(r);
            };
        } else if (expr instanceof Case k) {
            return TsExpr.when(
                    lowerPredicate(k.when()), lowerScalar(k.then()), lowerScalar(k.otherwise()));
        } else if (expr instanceof Compare || expr instanceof And
                || expr instanceof Or || expr instanceof Not) {
            throw TsSqlException.type("a boolean expression is not a scalar projection");
        } else {
            throw TsSqlException.type(
                    "an aggregate is not allowed in a row-wise projection (add GROUP BY)");
        }
    }

    private static TsExpr lowerPredicate(SqlExpr expr) {
        if (expr instanceof Compare c) {
            TsExpr l = lowerScalar(c.lhs());
            TsExpr r = lowerScalar(c.rhs());
            return switch (c.op()) {
                case EQ -> l.eq(r);
                case NE -> l.ne(r);
                case LT -> l.lt(r);
                case LE -> l.le(r);
                case GT -> l.gt(r);
                case GE -> l.ge(r);
            };
        } else if (expr instanceof And a) {
            // AND: true only where both 1/0 masks are true -> product > 0.
            TsExpr l = boolToInt(lowerPredicate(a.lhs()));
            TsExpr r = boolToInt(lowerPredicate(a.rhs()));
            return l.mul(r).gt(TsExpr.litI64(0));
        } else if (expr instanceof Or o) {
            // OR: true where either mask is true -> sum of the 1/0 masks > 0.
            TsExpr l = boolToInt(lowerPredicate(o.lhs()));
            TsExpr r = boolToInt(lowerPredicate(o.rhs()));
            return l.add(r).gt(TsExpr.litI64(0));
        } else if (expr instanceof Not n) {
            // NOT: the 1/0 mask equals 0.
            return boolToInt(lowerPredicate(n.operand())).eq(TsExpr.litI64(0));
        }
        throw TsSqlException.type("expected a boolean predicate, got a scalar");
    }

    private static TsExpr boolToInt(TsExpr pred) {
        return TsExpr.when(pred, TsExpr.litI64(1), TsExpr.litI64(0));
    }

    private static TsExpr lowerAggExpr(SqlExpr expr) {
        if (expr instanceof Aggregate a) {
            Optional<SqlExpr> arg = a.arg();
            TsExpr operand;
            if (arg.isPresent()) {
                operand = lowerScalar(arg.get());
            } else {
                // COUNT(*) counts rows; count over a constant 1 column is the row
                // count regardless of nulls in any data column.
                if (a.func() == AggFunc.COUNT) {
                    return TsExpr.litI64(1).count();
                }
                throw TsSqlException.type("only COUNT supports a * argument");
            }
            return switch (a.func()) {
                case SUM -> operand.sum();
                case AVG -> operand.mean();
                case MIN -> operand.min();
                case MAX -> operand.max();
                case COUNT -> operand.count();
            };
        }
        throw TsSqlException.type(
                "an aggregate projection must be a single aggregate call (e.g. SUM(x)); "
                        + "arithmetic over an aggregate is out of scope");
    }

    private static TsExpr lowerLiteral(SqlLiteral lit) {
        if (lit instanceof IntLit n) {
            return TsExpr.litI64(n.value());
        } else if (lit instanceof NumLit n) {
            return TsExpr.litF64(n.value());
        } else {
            return TsExpr.litStr(((StrLit) lit).value());
        }
    }

    // ---------- validation + error mapping ----------

    private static void validateColumns(TsSqlStmt stmt, List<String> sourceCols) {
        for (SelectItem item : stmt.projection()) {
            if (item instanceof ExprItem e) {
                checkExprColumns(e.expr(), sourceCols);
            }
        }
        if (stmt.filter() != null) {
            checkExprColumns(stmt.filter(), sourceCols);
        }
        for (String key : stmt.groupBy()) {
            if (!sourceCols.contains(key)) {
                throw TsSqlException.unknownColumn(key);
            }
        }
        for (OrderKey key : stmt.orderBy()) {
            // An ORDER BY key may name an output alias as well as a source
            // column; defer the alias case to exec time.
            if (!sourceCols.contains(key.column()) && !isOutputAlias(stmt, key.column())) {
                throw TsSqlException.unknownColumn(key.column());
            }
        }
    }

    private static boolean isOutputAlias(TsSqlStmt stmt, String name) {
        for (SelectItem item : stmt.projection()) {
            if (item instanceof ExprItem e && e.alias().equals(name)) {
                return true;
            }
        }
        return false;
    }

    private static void checkExprColumns(SqlExpr expr, List<String> sourceCols) {
        if (expr instanceof Column c) {
            if (!sourceCols.contains(c.name())) {
                throw TsSqlException.unknownColumn(c.name());
            }
        } else if (expr instanceof Literal) {
            // no columns
        } else if (expr instanceof Arith a) {
            checkExprColumns(a.lhs(), sourceCols);
            checkExprColumns(a.rhs(), sourceCols);
        } else if (expr instanceof Compare c) {
            checkExprColumns(c.lhs(), sourceCols);
            checkExprColumns(c.rhs(), sourceCols);
        } else if (expr instanceof And n) {
            checkExprColumns(n.lhs(), sourceCols);
            checkExprColumns(n.rhs(), sourceCols);
        } else if (expr instanceof Or n) {
            checkExprColumns(n.lhs(), sourceCols);
            checkExprColumns(n.rhs(), sourceCols);
        } else if (expr instanceof Not n) {
            checkExprColumns(n.operand(), sourceCols);
        } else if (expr instanceof Case k) {
            checkExprColumns(k.when(), sourceCols);
            checkExprColumns(k.then(), sourceCols);
            checkExprColumns(k.otherwise(), sourceCols);
        } else if (expr instanceof Aggregate a) {
            a.arg().ifPresent(inner -> checkExprColumns(inner, sourceCols));
        }
    }

    private static TsSqlException mapLazyError(RuntimeException e) {
        // An unknown sort column maps to UNKNOWN_COLUMN; anything else (an eval
        // type error from a malformed predicate / projection) is a type error.
        if (e instanceof LazyException le
                && le.kind() == LazyException.Kind.UNKNOWN_SORT_COLUMN) {
            return TsSqlException.unknownColumn(columnFromMessage(le.getMessage()));
        }
        return TsSqlException.type(e.getMessage());
    }

    private static TsSqlException mapGroupByError(GroupByException e) {
        if (e.kind() == GroupByException.Kind.UNKNOWN_COLUMN) {
            return TsSqlException.unknownColumn(columnFromMessage(e.getMessage()));
        }
        return TsSqlException.type(e.getMessage());
    }

    // Recover the column name from an upstream "... <column>" message.
    private static String columnFromMessage(String msg) {
        if (msg == null) {
            return "<unknown>";
        }
        int idx = msg.lastIndexOf(' ');
        return idx >= 0 ? msg.substring(idx + 1) : msg;
    }
}
