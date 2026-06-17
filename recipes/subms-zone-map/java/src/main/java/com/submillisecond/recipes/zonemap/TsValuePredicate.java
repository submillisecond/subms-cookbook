package com.submillisecond.recipes.zonemap;

/**
 * A single-sided value test {@code value OP rhs}. Mirrors the Rust
 * {@code TsValuePredicate}.
 */
public record TsValuePredicate(TsValueOp op, double rhs) {

    public static TsValuePredicate of(TsValueOp op, double rhs) {
        return new TsValuePredicate(op, rhs);
    }

    /**
     * Could ANY value in {@code [valueMin, valueMax]} satisfy the predicate?
     * Pruning is conservative: a {@code false} means "definitely cannot", so
     * the block is safe to skip; a {@code true} means "maybe", so the block is
     * read.
     */
    public boolean satisfiable(double valueMin, double valueMax) {
        return switch (op) {
            case LT -> valueMin < rhs;
            case LE -> valueMin <= rhs;
            case GT -> valueMax > rhs;
            case GE -> valueMax >= rhs;
            case EQ -> valueMin <= rhs && rhs <= valueMax;
        };
    }
}
