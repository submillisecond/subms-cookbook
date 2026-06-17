package com.submillisecond.recipes.tsexpr;

import com.submillisecond.recipes.ts.TsValue;

/**
 * The expression IR. A {@code TsExpr} is an immutable tree the evaluator walks
 * column-at-a-time over a {@link com.submillisecond.recipes.ts.TsDataFrame}.
 * The static factories ({@link #col}, the typed {@code lit*} family,
 * {@link #when}) and the fluent methods ({@link #add}, {@link #gt},
 * {@link #mean}, ...) are the ergonomic surface; the record variants are public
 * so downstream recipes (the lazy planner, groupby, window) can pattern-match
 * and rewrite the tree.
 *
 * <p>The IR is type-erased; a node's element type is inferred at eval time from
 * the frame's column types and the literal types, not declared here. The type
 * rules live in {@link Eval}.
 *
 * <p>Byte-equivalent to the Rust sibling's {@code TsExpr} enum: same variants,
 * same builder vocabulary, modulo case style.
 */
public sealed interface TsExpr
        permits TsExpr.Col, TsExpr.Lit, TsExpr.Unary, TsExpr.Binary,
                TsExpr.Compare, TsExpr.When, TsExpr.Agg {

    /** Reference a frame column by name. Its type is the column's dtype. */
    record Col(String name) implements TsExpr {}

    /** A typed scalar literal, broadcast to every row. */
    record Lit(TsValue value) implements TsExpr {}

    /** Elementwise unary op over its operand. Numeric only. */
    record Unary(TsUnaryOp op, TsExpr operand) implements TsExpr {}

    /** Elementwise binary op over its two operands. Numeric only. */
    record Binary(TsBinaryOp op, TsExpr lhs, TsExpr rhs) implements TsExpr {}

    /** Elementwise comparison; yields a Bool array. */
    record Compare(TsCmpOp op, TsExpr lhs, TsExpr rhs) implements TsExpr {}

    /** Elementwise select: where {@code cond} is true take {@code then}. */
    record When(TsExpr cond, TsExpr then, TsExpr otherwise) implements TsExpr {}

    /** Reduce the operand to a scalar, then broadcast it to every row. */
    record Agg(TsAggOp op, TsExpr operand) implements TsExpr {}

    // ---------- static factories ----------

    static TsExpr col(String name) {
        return new Col(name);
    }

    static TsExpr litF64(double value) {
        return new Lit(TsValue.ofDouble(value));
    }

    static TsExpr litI64(long value) {
        return new Lit(TsValue.ofLong(value));
    }

    static TsExpr litBool(boolean value) {
        return new Lit(TsValue.ofBool(value));
    }

    static TsExpr litStr(String value) {
        return new Lit(TsValue.ofString(value));
    }

    /** Elementwise select. Where {@code cond} is true take {@code then}, else
     *  {@code otherwise}; the two arms must share a type. */
    static TsExpr when(TsExpr cond, TsExpr then, TsExpr otherwise) {
        return new When(cond, then, otherwise);
    }

    // ---------- fluent builders ----------

    default TsExpr neg() {
        return new Unary(TsUnaryOp.NEG, this);
    }

    default TsExpr abs() {
        return new Unary(TsUnaryOp.ABS, this);
    }

    default TsExpr add(TsExpr rhs) {
        return new Binary(TsBinaryOp.ADD, this, rhs);
    }

    default TsExpr sub(TsExpr rhs) {
        return new Binary(TsBinaryOp.SUB, this, rhs);
    }

    default TsExpr mul(TsExpr rhs) {
        return new Binary(TsBinaryOp.MUL, this, rhs);
    }

    default TsExpr div(TsExpr rhs) {
        return new Binary(TsBinaryOp.DIV, this, rhs);
    }

    default TsExpr lt(TsExpr rhs) {
        return new Compare(TsCmpOp.LT, this, rhs);
    }

    default TsExpr le(TsExpr rhs) {
        return new Compare(TsCmpOp.LE, this, rhs);
    }

    default TsExpr eq(TsExpr rhs) {
        return new Compare(TsCmpOp.EQ, this, rhs);
    }

    default TsExpr ne(TsExpr rhs) {
        return new Compare(TsCmpOp.NE, this, rhs);
    }

    default TsExpr ge(TsExpr rhs) {
        return new Compare(TsCmpOp.GE, this, rhs);
    }

    default TsExpr gt(TsExpr rhs) {
        return new Compare(TsCmpOp.GT, this, rhs);
    }

    default TsExpr sum() {
        return new Agg(TsAggOp.SUM, this);
    }

    default TsExpr min() {
        return new Agg(TsAggOp.MIN, this);
    }

    default TsExpr max() {
        return new Agg(TsAggOp.MAX, this);
    }

    default TsExpr mean() {
        return new Agg(TsAggOp.MEAN, this);
    }

    default TsExpr count() {
        return new Agg(TsAggOp.COUNT, this);
    }
}
