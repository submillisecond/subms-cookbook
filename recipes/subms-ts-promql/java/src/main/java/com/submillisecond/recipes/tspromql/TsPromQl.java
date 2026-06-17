package com.submillisecond.recipes.tspromql;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.tspromql.Ast.Expr;

/**
 * A zero-dependency, hand-rolled PromQL-subset query engine over a
 * {@code TsCollection<Double>}. Parses a useful slice of PromQL (instant +
 * range selectors with label matchers, the {@code sum}/{@code avg}/{@code min}/
 * {@code max}/{@code count} aggregations with {@code by}/{@code without}
 * grouping, the {@code rate}/{@code irate}/{@code increase} range functions,
 * scalar/vector binary ops, and the {@code offset} modifier) and evaluates it
 * against the series in a collection, resolving a selector to the set of series
 * whose {@code __name__} (or metadata name) + label tags match.
 *
 * <p>Std-only: a char-cursor lexer, a recursive-descent parser, and a
 * tree-walking evaluator. {@code =~} / {@code !~} use {@link java.util.regex}
 * (JDK standard library) - the Rust sibling, having no std regex, restricts
 * those to literal + {@code .*}; that asymmetry is a documented non-claim.
 *
 * <p>Non-claims: subqueries, the {@code @} modifier, {@code histogram_quantile}
 * and the rest of the function library, staleness handling, {@code bool}
 * modifiers, {@code on}/{@code ignoring}/{@code group_left} vector matching, and
 * {@code topk}/{@code bottomk}/{@code quantile}.
 *
 * <pre>{@code
 * TsCollection<Double> coll = new TsCollection<>();
 * coll.register(TsSeriesMetadata.of(1, "")
 *     .withTag("__name__", "http_requests_total")
 *     .withTag("job", "api"));
 * coll.push(1, 1_000, 10.0);
 * coll.push(1, 2_000, 12.0);
 *
 * TsPromQl engine = new TsPromQl(coll);
 * TsPromQlResult res = engine.evalInstant("http_requests_total{job=\"api\"}", 2_000);
 * }</pre>
 */
public final class TsPromQl {

    private final PromQlEval eval;

    public TsPromQl(TsCollection<Double> coll) {
        this.eval = new PromQlEval(coll);
    }

    /** Parse + evaluate {@code query} at the instant {@code atTs} (i64 nanos). */
    public TsPromQlResult evalInstant(String query, long atTs) {
        Expr expr = Parser.parse(query);
        return eval.evalInstant(expr, atTs);
    }

    /**
     * Parse once, evaluate at every step in {@code [start, end]} advancing by
     * {@code step} nanos. {@code step} must be positive and
     * {@code start <= end}.
     */
    public TsPromQlRangeResult evalRange(String query, long start, long end, long step) {
        if (step <= 0) {
            throw TsPromQlException.eval("range step must be positive");
        }
        if (start > end) {
            throw TsPromQlException.eval("range start is after end");
        }
        Expr expr = Parser.parse(query);
        List<TsPromQlRangeResult.Step> steps = new ArrayList<>();
        long at = start;
        while (at <= end) {
            steps.add(new TsPromQlRangeResult.Step(at, eval.evalInstant(expr, at)));
            long next = at + step;
            if (next < at) {
                break; // overflow guard
            }
            at = next;
        }
        return new TsPromQlRangeResult(steps);
    }
}
