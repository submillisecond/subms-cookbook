package com.submillisecond.recipes.tspromql;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import com.submillisecond.recipes.tspromql.Ast.Expr;

/**
 * Drives a representative subms-ts-promql workload. Two stages mirror the Rust
 * recipe: {@code parse} repeatedly parses a moderate query; {@code eval}
 * evaluates {@code sum by (job) (rate(metric[5m]))} over a collection of a
 * couple hundred tagged counter series.
 */
public final class PromQlRecipe implements SubMsRecipe {

    private static final int SERIES = 200;
    private static final long POINTS = 32;
    private static final long STEP_NS = 15_000_000_000L; // 15s scrape interval
    private static final int JOBS = 6;

    private static final String PARSE_QUERY =
            "sum by (job) (rate(http_requests_total{job=~\"api.*\", code!=\"500\"}[5m]))"
            + " / count by (job) (http_requests_total{job=~\"api.*\"})";
    private static final String EVAL_QUERY = "sum by (job) (rate(http_requests_total[5m]))";

    private static TsCollection<Double> buildCollection() {
        TsCollection<Double> coll = new TsCollection<>();
        for (int i = 0; i < SERIES; i++) {
            coll.register(TsSeriesMetadata.of(i, "")
                    .withTag("__name__", "http_requests_total")
                    .withTag("job", "api-" + (i % JOBS))
                    .withTag("instance", "host-" + i)
                    .withTag("code", "200"));
            double slope = 1.0 + (i % 7);
            double acc = 0.0;
            for (long p = 0; p < POINTS; p++) {
                acc += slope;
                coll.push(i, p * STEP_NS, acc);
            }
        }
        return coll;
    }

    @Override
    public String name() {
        return "subms-ts-promql";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();
        long sink = 0;

        SubMsPerfHarness.Stage sParse = h.stage("parse", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            Expr expr = Parser.parse(PARSE_QUERY);
            sParse.record(SubMsTimer.nanosNow() - t0);
            sink += expr.hashCode();
        }

        TsCollection<Double> coll = buildCollection();
        long at = (POINTS - 1) * STEP_NS;
        TsPromQl engine = new TsPromQl(coll);
        SubMsPerfHarness.Stage sEval = h.stage("eval", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int r = 0; r < rounds; r++) {
            long t0 = SubMsTimer.nanosNow();
            TsPromQlResult res = engine.evalInstant(EVAL_QUERY, at);
            sEval.record(SubMsTimer.nanosNow() - t0);
            sink += res.size();
        }
        BLACK_HOLE = sink;

        h.meta("series", Integer.toString(SERIES));
        h.meta("points_per_series", Long.toString(POINTS));
        h.meta("subms.workload.feature", "promql");
    }

    static volatile long BLACK_HOLE;
}
