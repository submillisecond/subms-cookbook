package com.submillisecond.recipes.tspromql;

import java.util.List;
import java.util.regex.Pattern;

/**
 * The parsed query tree and its leaf types. Held as a sealed {@link Expr}
 * hierarchy; the evaluator pattern-matches on it. Range vectors are not a
 * top-level expr - they ride inside {@link Func} as the selector plus its
 * {@code rangeNs}.
 */
final class Ast {

    private Ast() {}

    enum MatchOp {
        EQ,
        NE,
        RE,
        NRE
    }

    enum AggOp {
        SUM,
        AVG,
        MIN,
        MAX,
        COUNT
    }

    enum FuncKind {
        RATE,
        IRATE,
        INCREASE
    }

    enum BinOp {
        ADD,
        SUB,
        MUL,
        DIV
    }

    enum GroupKind {
        NONE,
        BY,
        WITHOUT
    }

    /** One {@code name<op>"value"} label matcher inside a selector's braces. */
    record LabelMatcher(String label, MatchOp op, String pattern) {

        /**
         * Does {@code value} satisfy this matcher? {@code RE}/{@code NRE} go
         * through {@link Pattern} with whole-string anchoring (matching
         * PromQL's implicit {@code ^...$}). Unlike the Rust sibling - which
         * has no std regex and so restricts {@code =~} to literal + {@code .*}
         * - the JDK build accepts full {@code java.util.regex} syntax. The
         * recipe documents this asymmetry as a deliberate non-claim on the
         * Rust side.
         */
        boolean matches(String value) {
            return switch (op) {
                case EQ -> pattern.equals(value);
                case NE -> !pattern.equals(value);
                case RE -> value != null && fullMatch(value);
                case NRE -> !(value != null && fullMatch(value));
            };
        }

        private boolean fullMatch(String value) {
            return Pattern.matches(pattern, value);
        }
    }

    record Grouping(GroupKind kind, List<String> labels) {
        static final Grouping NONE = new Grouping(GroupKind.NONE, List.of());
    }

    record Selector(String metric, List<LabelMatcher> matchers, long offsetNs) {}

    /** Sealed expression tree. */
    sealed interface Expr permits Scalar, SelectorExpr, Func, Agg, Binary {}

    record Scalar(double value) implements Expr {}

    record SelectorExpr(Selector selector) implements Expr {}

    record Func(FuncKind kind, Selector selector, long rangeNs) implements Expr {}

    record Agg(AggOp op, Grouping grouping, Expr inner) implements Expr {}

    record Binary(BinOp op, Expr lhs, Expr rhs) implements Expr {}
}
