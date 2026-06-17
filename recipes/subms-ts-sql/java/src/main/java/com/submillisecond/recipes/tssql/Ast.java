package com.submillisecond.recipes.tssql;

import java.util.List;
import java.util.Optional;

/**
 * The parsed-statement IR. {@link TsSqlStmt} is the whole SELECT after the
 * parser has validated structure but before any lowering decision; the lowerer
 * in {@link Lower} walks it and decides whether the query is a grouped aggregate
 * (lower to {@code subms-ts-groupby}) or a row-wise projection pipeline (lower
 * to {@code subms-ts-lazy}). The nested types mirror the Rust sibling's
 * {@code ast} module.
 */
final class Ast {

    private Ast() {}

    /** A scalar literal as written in the SQL text. */
    sealed interface SqlLiteral permits IntLit, NumLit, StrLit {}

    /** An integer literal with no decimal point or exponent. */
    record IntLit(long value) implements SqlLiteral {}

    /** A floating literal. */
    record NumLit(double value) implements SqlLiteral {}

    /** A single-quoted string literal. */
    record StrLit(String value) implements SqlLiteral {}

    /** Arithmetic operators in a scalar expression. */
    enum ArithOp {
        ADD,
        SUB,
        MUL,
        DIV
    }

    /** Comparison operators in a predicate. */
    enum CmpOp {
        EQ,
        NE,
        LT,
        LE,
        GT,
        GE
    }

    /** The aggregate functions the subset supports. */
    enum AggFunc {
        SUM,
        AVG,
        MIN,
        MAX,
        COUNT
    }

    /**
     * A scalar / boolean expression. The same tree covers a projection scalar, a
     * WHERE predicate, and an aggregate operand; the lowerer enforces which
     * shapes are legal where. Comparison + boolean nodes only ever appear inside
     * a predicate.
     */
    sealed interface SqlExpr
            permits Column, Literal, Arith, Compare, And, Or, Not, Case, Aggregate {

        /**
         * Does this expression contain an aggregate call anywhere in its tree?
         * Drives the grouped-vs-rowwise lowering decision.
         */
        default boolean containsAggregate() {
            return switch (this) {
                case Aggregate a -> true;
                case Column c -> false;
                case Literal l -> false;
                case Arith a -> a.lhs().containsAggregate() || a.rhs().containsAggregate();
                case Compare c -> c.lhs().containsAggregate() || c.rhs().containsAggregate();
                case And n -> n.lhs().containsAggregate() || n.rhs().containsAggregate();
                case Or n -> n.lhs().containsAggregate() || n.rhs().containsAggregate();
                case Not n -> n.operand().containsAggregate();
                case Case k -> k.when().containsAggregate()
                        || k.then().containsAggregate()
                        || k.otherwise().containsAggregate();
            };
        }
    }

    /** A column reference. */
    record Column(String name) implements SqlExpr {}

    /** A literal scalar. */
    record Literal(SqlLiteral value) implements SqlExpr {}

    /** Binary arithmetic over two scalar sub-expressions. */
    record Arith(ArithOp op, SqlExpr lhs, SqlExpr rhs) implements SqlExpr {}

    /** A comparison; yields a boolean. Legal only inside a predicate. */
    record Compare(CmpOp op, SqlExpr lhs, SqlExpr rhs) implements SqlExpr {}

    /** Boolean AND of two predicates. */
    record And(SqlExpr lhs, SqlExpr rhs) implements SqlExpr {}

    /** Boolean OR of two predicates. */
    record Or(SqlExpr lhs, SqlExpr rhs) implements SqlExpr {}

    /** Boolean NOT of a predicate. */
    record Not(SqlExpr operand) implements SqlExpr {}

    /** {@code CASE WHEN <pred> THEN <e> ELSE <e> END}. Lowers to a When. */
    record Case(SqlExpr when, SqlExpr then, SqlExpr otherwise) implements SqlExpr {}

    /** An aggregate call. {@code arg} is empty for {@code COUNT(*)}. */
    record Aggregate(AggFunc func, Optional<SqlExpr> arg) implements SqlExpr {}

    /** One item in the SELECT projection list. */
    sealed interface SelectItem permits Star, ExprItem {}

    /** {@code *} - every source column, in source order. */
    record Star() implements SelectItem {}

    /** A projected expression with its output column name. */
    record ExprItem(SqlExpr expr, String alias) implements SelectItem {}

    /** One ORDER BY key. */
    record OrderKey(String column, boolean ascending) {}

    /** A parsed SELECT statement. Absent clauses are empty / {@code null}. */
    record TsSqlStmt(
            List<SelectItem> projection,
            String table,
            SqlExpr filter,
            List<String> groupBy,
            List<OrderKey> orderBy,
            Optional<Integer> limit) {}
}
