package com.submillisecond.recipes.tslazy;

import java.util.List;

import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * One step of a lazy pipeline. {@code Agg} is terminal (whole-frame reduction to
 * a one-row result); the rest are row-preserving or row-shaping transforms that
 * chain freely. The records are public so the optimiser can match and rewrite
 * them.
 *
 * <p>Byte-equivalent in shape to the Rust sibling's {@code PlanNode} enum.
 */
public sealed interface PlanNode
        permits PlanNode.Select, PlanNode.Filter, PlanNode.WithColumn,
                PlanNode.SortBy, PlanNode.Limit, PlanNode.Agg {

    /** A short stable kind tag, used by explain and the cost model. */
    String kind();

    /** Keep only the named columns, in the requested order. */
    record Select(List<String> columns) implements PlanNode {
        public Select {
            columns = List.copyOf(columns);
        }

        @Override
        public String kind() {
            return "select";
        }
    }

    /** Keep rows where {@code predicate} evaluates to a true Bool cell. */
    record Filter(TsExpr predicate) implements PlanNode {
        @Override
        public String kind() {
            return "filter";
        }
    }

    /** Append (or replace) a derived column {@code name} computed from {@code expr}. */
    record WithColumn(String name, TsExpr expr) implements PlanNode {
        @Override
        public String kind() {
            return "with_column";
        }
    }

    /** Reorder rows by {@code column}, ascending or descending. */
    record SortBy(String column, boolean ascending) implements PlanNode {
        @Override
        public String kind() {
            return "sort_by";
        }
    }

    /** Keep the first {@code n} rows. */
    record Limit(int n) implements PlanNode {
        @Override
        public String kind() {
            return "limit";
        }
    }

    /** Terminal: reduce each {@code (name, expr)} to a scalar, one-row result. */
    record Agg(List<NamedExpr> aggs) implements PlanNode {
        public Agg {
            aggs = List.copyOf(aggs);
        }

        @Override
        public String kind() {
            return "agg";
        }
    }

    /** A named aggregate expression - the unit of an {@link Agg} terminal. */
    record NamedExpr(String name, TsExpr expr) {}
}
