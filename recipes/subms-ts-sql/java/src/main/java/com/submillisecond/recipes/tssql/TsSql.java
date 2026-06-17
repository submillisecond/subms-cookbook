package com.submillisecond.recipes.tssql;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.tssql.Ast.TsSqlStmt;

/**
 * A zero-dependency, hand-rolled SQL-subset parser and executor over the typed
 * {@link TsDataFrame} engine. It parses a SELECT against a named catalog of
 * frames and runs it by LOWERING each clause onto the operator recipes already
 * in the arc: row-wise projection / filter / sort / limit ride a
 * {@code subms-ts-lazy} pipeline, GROUP BY + aggregates lower to
 * {@code subms-ts-groupby}, and every scalar / predicate / aggregate is a
 * {@code subms-ts-expr} IR tree. There is no execution engine of its own - SQL
 * is a front-end syntax over the analytical layer.
 *
 * <p>Std-only (JDK only): a char-cursor lexer, a recursive-descent parser, and a
 * clause lowerer. The SQL analogue of the {@code subms-ts-promql} recipe.
 *
 * <p>Supported subset:
 * <pre>{@code
 * SELECT <proj> FROM <table>
 *   [WHERE <predicate>]
 *   [GROUP BY <col, ...>]
 *   [ORDER BY <col> [ASC|DESC], ...]
 *   [LIMIT <n>]
 * }</pre>
 *
 * <p>{@code <proj>} items: {@code *}, a column, an arithmetic expression,
 * {@code expr AS alias}, {@code CASE WHEN <pred> THEN <e> ELSE <e> END}, and the
 * aggregates {@code SUM/AVG/MIN/MAX/COUNT(expr)} / {@code COUNT(*)}. Predicates
 * compose {@code = <> < <= > >=} with {@code AND}/{@code OR}/{@code NOT} and
 * parentheses. Keywords are case-insensitive; identifiers keep their case;
 * string literals are single-quoted; {@code --} starts a line comment.
 *
 * <pre>{@code
 * TsSqlCatalog cat = new TsSqlCatalog();
 * cat.register("trades", frame);
 * TsDataFrame out = TsSql.query(cat,
 *     "SELECT symbol, SUM(size) AS total FROM trades GROUP BY symbol");
 * }</pre>
 *
 * <p>Non-claims: a single-table analytical subset, not a SQL engine. Out of
 * scope: JOIN, HAVING, subqueries, window functions, set operations, DDL/DML,
 * and arithmetic wrapped around an aggregate ({@code SUM(x) / 2}). Each is
 * rejected with an explicit {@link TsSqlException} naming the gap rather than
 * silently ignored.
 *
 * <p>Byte-equivalent behaviour to the Rust sibling's {@code subms-ts-sql} crate:
 * same surface, same lowering, same result shape, modulo case style.
 */
public final class TsSql {

    private TsSql() {}

    /**
     * Parse + execute {@code sql} against {@code catalog}, returning the result
     * as a {@link TsDataFrame}. The result's columns are the projection's output
     * names, in SELECT-list order; an aggregate query emits one row per group
     * (key columns first, then the aggregate columns).
     */
    public static TsDataFrame query(TsSqlCatalog catalog, String sql) {
        TsSqlStmt stmt = Parser.parse(sql);
        TsDataFrame source = catalog.table(stmt.table())
                .orElseThrow(() -> TsSqlException.unknownTable(stmt.table()));
        return Lower.execute(stmt, source);
    }

    /**
     * Parse {@code sql} into a statement IR without touching a catalog. Exposed
     * so the parser can be exercised independently of execution.
     */
    static TsSqlStmt parse(String sql) {
        return Parser.parse(sql);
    }
}
