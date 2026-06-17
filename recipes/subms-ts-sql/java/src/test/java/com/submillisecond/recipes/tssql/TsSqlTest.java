package com.submillisecond.recipes.tssql;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsgroupby.GroupBy;
import com.submillisecond.recipes.tsgroupby.TsGroupBy;
import com.submillisecond.recipes.tsgroupby.TsGroupResult;
import com.submillisecond.recipes.tssql.Ast.AggFunc;
import com.submillisecond.recipes.tssql.Ast.Aggregate;
import com.submillisecond.recipes.tssql.Ast.And;
import com.submillisecond.recipes.tssql.Ast.Case;
import com.submillisecond.recipes.tssql.Ast.Column;
import com.submillisecond.recipes.tssql.Ast.Compare;
import com.submillisecond.recipes.tssql.Ast.ExprItem;
import com.submillisecond.recipes.tssql.Ast.Or;
import com.submillisecond.recipes.tssql.Ast.SelectItem;
import com.submillisecond.recipes.tssql.Ast.StrLit;
import com.submillisecond.recipes.tssql.Ast.TsSqlStmt;

class TsSqlTest {

    // ---------- fixtures ----------

    private static TsDataFrame tradesFrame() {
        Object[][] rows = {
            {"AAPL", 10.0, 190.0},
            {"MSFT", 5.0, 410.0},
            {"AAPL", 7.0, 192.0},
            {"MSFT", 3.0, 408.0},
            {"AAPL", 4.0, 188.0},
            {"GOOG", 2.0, 140.0},
        };
        TsSeries<String> symbol = new TsSeries<>();
        TsSeriesD size = new TsSeriesD();
        TsSeriesD price = new TsSeriesD();
        for (int i = 0; i < rows.length; i++) {
            symbol.push(i, (String) rows[i][0]);
            size.push(i, (Double) rows[i][1]);
            price.push(i, (Double) rows[i][2]);
        }
        return new TsDataFrame()
                .withColumn("symbol", new TsColumn.Str(symbol))
                .withColumn("size", new TsColumn.F64(size))
                .withColumn("price", new TsColumn.F64(price));
    }

    private static TsSqlCatalog catalog() {
        TsSqlCatalog cat = new TsSqlCatalog();
        cat.register("trades", tradesFrame());
        return cat;
    }

    private static Optional<TsValue> cell(TsDataFrame frame, String col, long row) {
        return frame.column(col).flatMap(c -> c.get(row));
    }

    private static double f64At(TsDataFrame frame, String col, long row) {
        return ((TsValue.F64) cell(frame, col, row).orElseThrow()).value();
    }

    private static long i64At(TsDataFrame frame, String col, long row) {
        return ((TsValue.I64) cell(frame, col, row).orElseThrow()).value();
    }

    private static String strAt(TsDataFrame frame, String col, long row) {
        return ((TsValue.Str) cell(frame, col, row).orElseThrow()).value();
    }

    private static int nrows(TsDataFrame frame, String col) {
        return frame.column(col).map(TsColumn::len).orElse(0);
    }

    // ---------- parser tests ----------

    @Test
    void parseMinimalSelect() {
        TsSqlStmt stmt = TsSql.parse("SELECT symbol FROM trades");
        assertEquals("trades", stmt.table());
        assertEquals(1, stmt.projection().size());
        assertEquals(null, stmt.filter());
        assertTrue(stmt.groupBy().isEmpty());
        assertTrue(stmt.orderBy().isEmpty());
        assertTrue(stmt.limit().isEmpty());
    }

    @Test
    void parseIsCaseInsensitiveAndPreservesIdentifiers() {
        TsSqlStmt stmt = TsSql.parse(
                "select Symbol from trades where price > 100 order by price desc limit 3");
        SelectItem item = stmt.projection().get(0);
        assertTrue(item instanceof ExprItem);
        ExprItem e = (ExprItem) item;
        assertEquals(new Column("Symbol"), e.expr());
        assertEquals("Symbol", e.alias());
        assertEquals(1, stmt.orderBy().size());
        assertFalse(stmt.orderBy().get(0).ascending());
        assertEquals(Optional.of(3), stmt.limit());
    }

    @Test
    void parseHandlesLineCommentsAndStringLiterals() {
        String sql = "SELECT symbol -- the ticker\nFROM trades\nWHERE symbol = 'AA''PL'";
        TsSqlStmt stmt = TsSql.parse(sql);
        Compare cmp = (Compare) stmt.filter();
        assertEquals(new StrLit("AA'PL"), ((Ast.Literal) cmp.rhs()).value());
    }

    @Test
    void parseAggregatesAndStarCount() {
        TsSqlStmt stmt = TsSql.parse(
                "SELECT symbol, SUM(size) AS s, COUNT(*) AS n FROM trades GROUP BY symbol");
        assertEquals(List.of("symbol"), stmt.groupBy());
        Aggregate sum = (Aggregate) ((ExprItem) stmt.projection().get(1)).expr();
        assertEquals(AggFunc.SUM, sum.func());
        assertTrue(sum.arg().isPresent());
        Aggregate count = (Aggregate) ((ExprItem) stmt.projection().get(2)).expr();
        assertEquals(AggFunc.COUNT, count.func());
        assertTrue(count.arg().isEmpty());
    }

    @Test
    void parseCaseAndBooleanPredicate() {
        TsSqlStmt stmt = TsSql.parse(
                "SELECT CASE WHEN price > 200 THEN 1 ELSE 0 END AS hi FROM trades "
                + "WHERE price > 100 AND NOT symbol = 'GOOG'");
        assertTrue(((ExprItem) stmt.projection().get(0)).expr() instanceof Case);
        assertTrue(stmt.filter() instanceof And);
    }

    @Test
    void parseRejectsGarbageAndTrailingTokens() {
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT"));
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT FROM trades"));
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT a FROM trades EXTRA"));
        assertEquals(TsSqlException.Kind.PARSE, parseErr(""));
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT a FROM trades WHERE 'unterminated"));
    }

    @Test
    void parseRejectsUnsupportedClausesByName() {
        assertEquals(TsSqlException.Kind.UNSUPPORTED, parseErr("SELECT a FROM t JOIN u"));
        assertEquals(
                TsSqlException.Kind.UNSUPPORTED,
                parseErr("SELECT a FROM t GROUP BY a HAVING COUNT(*) > 1"));
        assertEquals(TsSqlException.Kind.UNSUPPORTED, parseErr("SELECT a FROM t LEFT JOIN u"));
    }

    private static TsSqlException.Kind parseErr(String sql) {
        return assertThrows(TsSqlException.class, () -> TsSql.parse(sql)).kind();
    }

    private static TsSqlException.Kind queryErr(String sql) {
        return assertThrows(TsSqlException.class, () -> TsSql.query(catalog(), sql)).kind();
    }

    // ---------- executor tests ----------

    @Test
    void selectStarReturnsEveryColumn() {
        TsDataFrame out = TsSql.query(catalog(), "SELECT * FROM trades");
        assertEquals(List.of("symbol", "size", "price"), out.columnNames());
        assertEquals(6, nrows(out, "symbol"));
    }

    @Test
    void whereFiltersRows() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, price FROM trades WHERE price > 200");
        assertEquals(2, nrows(out, "symbol"));
        for (long r = 0; r < 2; r++) {
            assertEquals("MSFT", strAt(out, "symbol", r));
        }
    }

    @Test
    void whereCompoundAndOrNot() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol FROM trades WHERE price > 189 AND NOT symbol = 'AAPL'");
        assertEquals(2, nrows(out, "symbol"));

        TsDataFrame out2 = TsSql.query(
                catalog(), "SELECT symbol FROM trades WHERE symbol = 'GOOG' OR price > 409");
        assertEquals(2, nrows(out2, "symbol"));
    }

    @Test
    void projectionArithmeticAndAlias() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, size * price AS notional FROM trades WHERE symbol = 'GOOG'");
        assertEquals(1, nrows(out, "notional"));
        assertEquals(2.0 * 140.0, f64At(out, "notional", 0));
    }

    @Test
    void caseWhenLowersToConditional() {
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, CASE WHEN price > 200 THEN 1 ELSE 0 END AS hi FROM trades");
        long sum = 0;
        for (long r = 0; r < 6; r++) {
            sum += i64At(out, "hi", r);
        }
        assertEquals(2, sum);
    }

    @Test
    void orderByAndLimitPreserveSortOrder() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, price FROM trades ORDER BY price DESC LIMIT 3");
        assertEquals(3, nrows(out, "price"));
        assertEquals(410.0, f64At(out, "price", 0));
        assertEquals(408.0, f64At(out, "price", 1));
        assertEquals(192.0, f64At(out, "price", 2));
    }

    @Test
    void groupByAggregatesSumAvgCount() {
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, SUM(size) AS total, AVG(price) AS avg_px, COUNT(*) AS n "
                + "FROM trades GROUP BY symbol");
        assertEquals("AAPL", strAt(out, "symbol", 0));
        assertEquals(21.0, f64At(out, "total", 0));
        assertEquals((190.0 + 192.0 + 188.0) / 3.0, f64At(out, "avg_px", 0));
        assertEquals(3, i64At(out, "n", 0));
        assertEquals("MSFT", strAt(out, "symbol", 2));
        assertEquals(8.0, f64At(out, "total", 2));
    }

    @Test
    void groupByMinMax() {
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, MIN(price) AS lo, MAX(price) AS hi FROM trades GROUP BY symbol");
        assertEquals(188.0, f64At(out, "lo", 0));
        assertEquals(192.0, f64At(out, "hi", 0));
    }

    @Test
    void aggregateWithoutGroupByIsWholeFrame() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT COUNT(*) AS n, SUM(size) AS total FROM trades");
        assertEquals(1, nrows(out, "n"));
        assertEquals(6, i64At(out, "n", 0));
        assertEquals(31.0, f64At(out, "total", 0));
        assertEquals(List.of("n", "total"), out.columnNames());
    }

    @Test
    void groupByWithWhereOrderLimitComposes() {
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, SUM(size) AS total FROM trades WHERE price > 150 "
                + "GROUP BY symbol ORDER BY total DESC LIMIT 1");
        assertEquals(1, nrows(out, "total"));
        assertEquals("AAPL", strAt(out, "symbol", 0));
        assertEquals(21.0, f64At(out, "total", 0));
    }

    @Test
    void groupByLoweringMatchesGroupByDirectly() {
        TsDataFrame frame = tradesFrame();
        TsGroupResult direct = GroupBy.groupBy(frame, "symbol").agg(
                new TsGroupBy.Agg("total", TsExpr.col("size").sum()),
                new TsGroupBy.Agg("n", TsExpr.litI64(1).count()));

        TsDataFrame viaSql = TsSql.query(
                catalog(),
                "SELECT symbol, SUM(size) AS total, COUNT(*) AS n FROM trades GROUP BY symbol");

        assertEquals(direct.nrows(), nrows(viaSql, "symbol"));
        for (int g = 0; g < direct.nrows(); g++) {
            assertEquals(direct.value("symbol", g), cell(viaSql, "symbol", g));
            assertEquals(direct.value("total", g), cell(viaSql, "total", g));
            assertEquals(direct.value("n", g), cell(viaSql, "n", g));
        }
    }

    @Test
    void stringKeyGroupByWorks() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, COUNT(*) AS n FROM trades GROUP BY symbol");
        assertEquals(3, nrows(out, "symbol"));
        assertEquals("AAPL", strAt(out, "symbol", 0));
        assertEquals(3, i64At(out, "n", 0));
        assertEquals("GOOG", strAt(out, "symbol", 1));
        assertEquals(1, i64At(out, "n", 1));
    }

    // ---------- negative / error tests ----------

    @Test
    void unknownTableIsTypedError() {
        assertEquals(TsSqlException.Kind.UNKNOWN_TABLE, queryErr("SELECT a FROM nope"));
    }

    @Test
    void unknownColumnIsTypedError() {
        assertEquals(TsSqlException.Kind.UNKNOWN_COLUMN, queryErr("SELECT missing FROM trades"));
        assertEquals(
                TsSqlException.Kind.UNKNOWN_COLUMN,
                queryErr("SELECT symbol FROM trades WHERE ghost > 1"));
    }

    @Test
    void nonKeyNonAggProjectionWithGroupByIsRejected() {
        assertEquals(
                TsSqlException.Kind.TYPE,
                queryErr("SELECT symbol, price FROM trades GROUP BY symbol"));
    }

    @Test
    void starWithGroupByIsRejected() {
        assertEquals(TsSqlException.Kind.TYPE, queryErr("SELECT * FROM trades GROUP BY symbol"));
    }

    @Test
    void catalogTableNamesAndLookup() {
        TsSqlCatalog cat = catalog();
        assertEquals(List.of("trades"), cat.tableNames());
        assertTrue(cat.table("trades").isPresent());
        assertTrue(cat.table("other").isEmpty());
    }

    @Test
    void parseIsExposedForIndependentTesting() {
        TsSqlStmt stmt = TsSql.parse("SELECT a, b FROM t LIMIT 10");
        assertEquals(Optional.of(10), stmt.limit());
        assertEquals(2, stmt.projection().size());
    }

    // ---------- extra coverage: operators, literals, shapes ----------

    @Test
    void allComparisonOperatorsParseAndExecute() {
        // <, <=, <>, >=, != each filter; the != / <> pair are equivalent.
        assertEquals(1, nrows(TsSql.query(catalog(),
                "SELECT symbol FROM trades WHERE price < 150"), "symbol")); // GOOG only
        assertEquals(1, nrows(TsSql.query(catalog(),
                "SELECT symbol FROM trades WHERE price <= 140"), "symbol")); // GOOG
        assertEquals(5, nrows(TsSql.query(catalog(),
                "SELECT symbol FROM trades WHERE symbol <> 'GOOG'"), "symbol"));
        assertEquals(5, nrows(TsSql.query(catalog(),
                "SELECT symbol FROM trades WHERE symbol != 'GOOG'"), "symbol"));
        assertEquals(2, nrows(TsSql.query(catalog(),
                "SELECT symbol FROM trades WHERE price >= 408"), "symbol")); // two MSFT
    }

    @Test
    void floatLiteralAndDivisionInProjection() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, price / 2.0 AS half FROM trades WHERE symbol = 'GOOG'");
        assertEquals(70.0, f64At(out, "half", 0));
    }

    @Test
    void unaryMinusLiteralAndExpression() {
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, -price AS negp FROM trades WHERE symbol = 'GOOG'");
        assertEquals(-140.0, f64At(out, "negp", 0));
        // unary minus on a non-literal lowers to 0 - x.
        TsDataFrame out2 = TsSql.query(
                catalog(),
                "SELECT symbol, -(size * price) AS negn FROM trades WHERE symbol = 'GOOG'");
        assertEquals(-(2.0 * 140.0), f64At(out2, "negn", 0));
    }

    @Test
    void parenthesisedPredicateGroups() {
        // (price > 409 OR symbol = 'GOOG') AND NOT symbol = 'AAPL'
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol FROM trades WHERE (price > 409 OR symbol = 'GOOG') "
                + "AND NOT symbol = 'AAPL'");
        assertEquals(2, nrows(out, "symbol")); // the 410 MSFT + GOOG
    }

    @Test
    void multiKeyOrderByWithExplicitAsc() {
        // ORDER BY symbol ASC, price DESC.
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, price FROM trades ORDER BY symbol ASC, price DESC");
        assertEquals("AAPL", strAt(out, "symbol", 0));
        assertEquals(192.0, f64At(out, "price", 0)); // AAPL's highest first
        assertEquals("GOOG", strAt(out, "symbol", 3));
    }

    @Test
    void minMaxOverStringKeyProducesStringAggregate() {
        // MIN / MAX over a Str column reduce lexicographically.
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, MIN(symbol) AS lo FROM trades GROUP BY symbol");
        assertEquals("AAPL", strAt(out, "lo", 0));
    }

    @Test
    void orderByOutputAliasAfterAggregate() {
        // ORDER BY references the aggregate's alias, not a source column.
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, SUM(size) AS total FROM trades GROUP BY symbol ORDER BY total ASC");
        assertEquals(2.0, f64At(out, "total", 0)); // GOOG smallest
    }

    @Test
    void badLimitIsParseError() {
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT a FROM t LIMIT x"));
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT a FROM t LIMIT -1"));
    }

    @Test
    void booleanProjectionInProjectionIsParseError() {
        // A bare comparison in projection position is not a scalar grammar
        // element: parseExpr stops at `>`, so the `> 100` is a trailing token.
        assertEquals(TsSqlException.Kind.PARSE, queryErr("SELECT price > 100 AS hi FROM trades"));
    }

    @Test
    void caseProducingStringValuesLowersStrColumn() {
        // CASE arms are strings -> the projected column is a Str array, exercising
        // the Str positional-column branch.
        TsDataFrame out = TsSql.query(
                catalog(),
                "SELECT symbol, CASE WHEN price > 200 THEN 'hi' ELSE 'lo' END AS band FROM trades "
                + "WHERE symbol = 'MSFT'");
        assertEquals("hi", strAt(out, "band", 0));
    }

    @Test
    void aggregateInRowwiseProjectionIsRejected() {
        // SUM with no GROUP BY and a sibling bare column forces an aggregate
        // query, but a lone aggregate wrapped in arithmetic is out of scope.
        assertEquals(
                TsSqlException.Kind.TYPE,
                queryErr("SELECT SUM(size) / 2 AS half FROM trades"));
    }

    @Test
    void emptyStringAggregateExceptionKinds() {
        // Exercise the exception factory surface directly.
        assertEquals(TsSqlException.Kind.PARSE, TsSqlException.parse("x").kind());
        assertEquals(TsSqlException.Kind.UNKNOWN_TABLE, TsSqlException.unknownTable("t").kind());
        assertEquals(TsSqlException.Kind.UNKNOWN_COLUMN, TsSqlException.unknownColumn("c").kind());
        assertEquals(TsSqlException.Kind.UNSUPPORTED, TsSqlException.unsupported("X").kind());
        assertEquals(TsSqlException.Kind.TYPE, TsSqlException.type("y").kind());
    }

    @Test
    void groupByAggregateOverUnknownColumnInExpr() {
        assertEquals(
                TsSqlException.Kind.UNKNOWN_COLUMN,
                queryErr("SELECT symbol, SUM(ghost) AS s FROM trades GROUP BY symbol"));
    }

    @Test
    void orderByDroppedColumnAfterAggregateIsUnknownColumn() {
        // `price` passes validation (a source column) but is absent from the
        // aggregated frame, so the lazy sort raises UNKNOWN_SORT_COLUMN, which the
        // lowerer maps to UNKNOWN_COLUMN.
        assertEquals(
                TsSqlException.Kind.UNKNOWN_COLUMN,
                queryErr("SELECT symbol, SUM(size) AS total FROM trades GROUP BY symbol "
                        + "ORDER BY price"));
    }

    @Test
    void reservedKeywordAsIdentifierIsParseError() {
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT symbol FROM select"));
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT FROM FROM trades"));
    }

    @Test
    void unbalancedParenthesisIsParseError() {
        assertEquals(TsSqlException.Kind.PARSE, parseErr("SELECT (size * price FROM trades"));
    }

    @Test
    void countOverExpressionCountsPresentValues() {
        // COUNT(price) (not COUNT(*)) counts present cells of the operand.
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, COUNT(price) AS n FROM trades GROUP BY symbol");
        assertEquals(3, i64At(out, "n", 0)); // AAPL has 3 rows
    }

    @Test
    void aggregateOverUnaryNegatedColumn() {
        // SUM(-size) exercises the unary-minus-of-column lowering (0 - x) inside
        // an aggregate operand.
        TsDataFrame out = TsSql.query(
                catalog(), "SELECT symbol, SUM(-size) AS negtotal FROM trades GROUP BY symbol");
        assertEquals(-21.0, f64At(out, "negtotal", 0)); // AAPL: -(10+7+4)
    }

    @Test
    void parserParseExposedReturnsStmtForEachClause() {
        TsSqlStmt stmt = TsSql.parse(
                "SELECT a, b * c AS d FROM t WHERE a >= 1 OR b < 2 "
                + "GROUP BY a ORDER BY d ASC, a DESC LIMIT 4");
        assertEquals("t", stmt.table());
        assertTrue(stmt.filter() instanceof Or);
        assertEquals(List.of("a"), stmt.groupBy());
        assertEquals(2, stmt.orderBy().size());
        assertTrue(stmt.orderBy().get(0).ascending());
        assertFalse(stmt.orderBy().get(1).ascending());
        assertEquals(Optional.of(4), stmt.limit());
    }
}
