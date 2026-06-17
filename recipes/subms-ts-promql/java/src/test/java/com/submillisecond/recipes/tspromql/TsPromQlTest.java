package com.submillisecond.recipes.tspromql;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Map;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

class TsPromQlTest {

    private static final long S = 1_000_000_000L;

    private static TsCollection<Double> fixture() {
        TsCollection<Double> coll = new TsCollection<>();
        register(coll, 1, "api", "i-1", new double[][] {{0, 0}, {60 * S, 60}, {120 * S, 120}});
        register(coll, 2, "api", "i-2", new double[][] {{0, 0}, {60 * S, 30}, {120 * S, 60}});
        register(coll, 3, "web", "i-3", new double[][] {{0, 0}, {60 * S, 10}, {120 * S, 20}});
        return coll;
    }

    private static void register(TsCollection<Double> coll, long id, String job, String inst, double[][] pts) {
        coll.register(TsSeriesMetadata.of(id, "")
                .withTag("__name__", "http_requests_total")
                .withTag("job", job)
                .withTag("instance", inst));
        for (double[] p : pts) {
            coll.push(id, (long) p[0], p[1]);
        }
    }

    @Test
    void instantSelectorEqMatcherResolvesRightSeries() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("http_requests_total{job=\"web\"}", 120 * S);
        assertEquals(1, res.size());
        assertEquals(20.0, res.samples().get(0).value());
        assertEquals("i-3", res.samples().get(0).labels().get("instance"));
    }

    @Test
    void bareSelectorResolvesAllSeries() {
        TsPromQl q = new TsPromQl(fixture());
        assertEquals(3, q.evalInstant("http_requests_total", 120 * S).size());
    }

    @Test
    void neMatcherExcludes() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("http_requests_total{job!=\"web\"}", 120 * S);
        assertEquals(2, res.size());
        assertTrue(res.samples().stream().allMatch(s -> s.labels().get("job").equals("api")));
    }

    @Test
    void reMatcherWithPattern() {
        TsPromQl q = new TsPromQl(fixture());
        assertEquals(2, q.evalInstant("http_requests_total{job=~\"a.*\"}", 120 * S).size());
        // JDK regex: anchored full match. i-.*2 matches instance i-2 only.
        TsPromQlResult res2 = q.evalInstant("http_requests_total{instance=~\"i-.*2\"}", 120 * S);
        assertEquals(1, res2.size());
        assertEquals("i-2", res2.samples().get(0).labels().get("instance"));
    }

    @Test
    void nreMatcherNegates() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("http_requests_total{job!~\"a.*\"}", 120 * S);
        assertEquals(1, res.size());
        assertEquals("web", res.samples().get(0).labels().get("job"));
    }

    @Test
    void sumByJobGroupsAndSums() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("sum by (job) (http_requests_total)", 120 * S);
        assertEquals(2, res.size());
        assertEquals(180.0, res.valueFor(Map.of("job", "api")).getAsDouble());
        assertEquals(20.0, res.valueFor(Map.of("job", "web")).getAsDouble());
    }

    @Test
    void avgMinMaxCountAggregations() {
        TsPromQl q = new TsPromQl(fixture());
        long at = 120 * S;
        assertEquals(90.0, q.evalInstant("avg by (job) (http_requests_total)", at)
                .valueFor(Map.of("job", "api")).getAsDouble());
        assertEquals(60.0, q.evalInstant("min by (job) (http_requests_total)", at)
                .valueFor(Map.of("job", "api")).getAsDouble());
        assertEquals(120.0, q.evalInstant("max by (job) (http_requests_total)", at)
                .valueFor(Map.of("job", "api")).getAsDouble());
        assertEquals(2.0, q.evalInstant("count by (job) (http_requests_total)", at)
                .valueFor(Map.of("job", "api")).getAsDouble());
    }

    @Test
    void aggregationWithout() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("sum without (instance) (http_requests_total)", 120 * S);
        assertEquals(2, res.size());
        assertEquals(180.0, res.valueFor(Map.of("job", "api")).getAsDouble());
    }

    @Test
    void aggregationNoGroupingCollapsesAll() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("sum(http_requests_total)", 120 * S);
        assertEquals(1, res.size());
        assertEquals(200.0, res.samples().get(0).value());
        assertTrue(res.samples().get(0).labels().isEmpty());
    }

    @Test
    void rateOverCounterGivesPerSecondSlope() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("rate(http_requests_total{instance=\"i-1\"}[5m])", 120 * S);
        assertEquals(1, res.size());
        assertEquals(1.0, res.samples().get(0).value(), 1e-9);

        TsPromQlResult res2 = q.evalInstant("rate(http_requests_total{instance=\"i-2\"}[5m])", 120 * S);
        assertEquals(0.5, res2.samples().get(0).value(), 1e-9);
    }

    @Test
    void irateUsesLastTwoSamples() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("irate(http_requests_total{instance=\"i-1\"}[5m])", 120 * S);
        assertEquals(1.0, res.samples().get(0).value(), 1e-9);
    }

    @Test
    void increaseIsTotalDelta() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("increase(http_requests_total{instance=\"i-1\"}[5m])", 120 * S);
        assertEquals(120.0, res.samples().get(0).value(), 1e-9);
    }

    @Test
    void binaryOpDivisionWithLabelMatching() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant(
                "sum by (job) (http_requests_total) / count by (job) (http_requests_total)", 120 * S);
        assertEquals(90.0, res.valueFor(Map.of("job", "api")).getAsDouble());
        assertEquals(20.0, res.valueFor(Map.of("job", "web")).getAsDouble());
    }

    @Test
    void scalarBinaryBroadcast() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("sum(http_requests_total) / 2", 120 * S);
        assertEquals(100.0, res.samples().get(0).value());
    }

    @Test
    void offsetShiftsEvalPoint() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("http_requests_total{instance=\"i-1\"} offset 1m", 120 * S);
        assertEquals(60.0, res.samples().get(0).value());
    }

    @Test
    void unknownMetricIsEmpty() {
        TsPromQl q = new TsPromQl(fixture());
        assertTrue(q.evalInstant("nonexistent_metric", 120 * S).isEmpty());
    }

    @Test
    void parseErrorOnMalformedQuery() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlException e1 = assertThrows(TsPromQlException.class,
                () -> q.evalInstant("sum by (job (http_requests_total)", 0));
        assertEquals(TsPromQlException.Kind.PARSE, e1.kind());

        TsPromQlException e2 = assertThrows(TsPromQlException.class,
                () -> q.evalInstant("http_requests_total{job=}", 0));
        assertEquals(TsPromQlException.Kind.PARSE, e2.kind());

        TsPromQlException e3 = assertThrows(TsPromQlException.class,
                () -> q.evalInstant("", 0));
        assertEquals(TsPromQlException.Kind.PARSE, e3.kind());
    }

    @Test
    void rangeEvalStepsOverWindow() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlRangeResult res = q.evalRange("sum(http_requests_total)", 0, 120 * S, 60 * S);
        assertEquals(3, res.size());
        assertEquals(0.0, res.steps().get(0).result().samples().get(0).value());
        assertEquals(200.0, res.steps().get(2).result().samples().get(0).value());
    }

    @Test
    void rangeEvalRejectsBadArgs() {
        TsPromQl q = new TsPromQl(fixture());
        assertEquals(TsPromQlException.Kind.EVAL, assertThrows(TsPromQlException.class,
                () -> q.evalRange("http_requests_total", 0, 10, 0)).kind());
        assertEquals(TsPromQlException.Kind.EVAL, assertThrows(TsPromQlException.class,
                () -> q.evalRange("http_requests_total", 10, 0, 1)).kind());
    }

    @Test
    void durationUnitsParse() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("rate(http_requests_total{instance=\"i-1\"}[1h])", 120 * S);
        assertEquals(1.0, res.samples().get(0).value(), 1e-9);
        assertEquals(86_400L * S, Parser.parseDurationNs("1d"));
        assertEquals(90L * S, Parser.parseDurationNs("1m30s"));
    }

    @Test
    void counterResetHandledInRate() {
        TsCollection<Double> coll = new TsCollection<>();
        coll.register(TsSeriesMetadata.of(1, "").withTag("__name__", "c").withTag("job", "x"));
        coll.push(1, 0, 0.0);
        coll.push(1, 10 * S, 100.0);
        coll.push(1, 20 * S, 10.0);
        coll.push(1, 30 * S, 30.0);
        TsPromQl q = new TsPromQl(coll);
        TsPromQlResult res = q.evalInstant("rate(c[5m])", 30 * S);
        assertEquals(130.0 / 30.0, res.samples().get(0).value(), 1e-9);
    }

    @Test
    void scalarResultAndEmptiness() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("sum(http_requests_total)", 120 * S);
        assertTrue(res.scalar().isPresent());
        assertEquals(200.0, res.scalar().get());
        assertFalse(res.isEmpty());
        // a multi-sample result has no scalar; a missing label set has no value.
        TsPromQlResult multi = q.evalInstant("sum by (job) (http_requests_total)", 120 * S);
        assertTrue(multi.scalar().isEmpty());
        assertTrue(multi.valueFor(Map.of("job", "nope")).isEmpty());
    }

    @Test
    void tsSampleEqualityHashAndString() {
        TsSample a = new TsSample(Map.of("job", "api"), 1.0);
        TsSample b = new TsSample(Map.of("job", "api"), 1.0);
        TsSample c = new TsSample(Map.of("job", "web"), 1.0);
        TsSample d = new TsSample(Map.of("job", "api"), 2.0);
        assertEquals(a, a);
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        assertFalse(a.equals(c));
        assertFalse(a.equals(d));
        assertFalse(a.equals("not a sample"));
        assertTrue(a.toString().contains("api"));
        assertEquals(1.0, a.value());
    }

    @Test
    void exceptionKindAccessor() {
        TsPromQlException p = TsPromQlException.parse("x");
        TsPromQlException e = TsPromQlException.eval("y");
        assertEquals(TsPromQlException.Kind.PARSE, p.kind());
        assertEquals(TsPromQlException.Kind.EVAL, e.kind());
        assertTrue(p.getMessage().contains("parse"));
        assertTrue(e.getMessage().contains("eval"));
    }

    @Test
    void parserErrorBranches() {
        // unterminated string
        assertParse("http_requests_total{job=\"oops");
        // lone bang
        assertParse("http_requests_total{job!\"x\"}");
        // unexpected character
        assertParse("http_requests_total & 1");
        // bare metric missing where an expression is expected
        assertParse("+");
        // function without a range bracket
        assertParse("rate(http_requests_total)");
        // function with a number where a duration belongs
        assertParse("rate(http_requests_total[5])");
        // offset without a duration
        assertParse("http_requests_total offset 5");
        // grouping with a non-label token
        assertParse("sum by (1) (http_requests_total)");
        // missing closing paren on a parenthesised group
        assertParse("(http_requests_total");
        // matcher value not a string
        assertParse("http_requests_total{job=5}");
        // duration with a bad unit slips through as an ident -> selector + junk
        assertParse("http_requests_total[5x]");
    }

    @Test
    void parenthesisedExpressionAndMixedDurations() {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlResult res = q.evalInstant("(sum(http_requests_total)) - 100", 120 * S);
        assertEquals(100.0, res.samples().get(0).value());
        // additive then multiplicative precedence
        TsPromQlResult res2 = q.evalInstant("sum(http_requests_total) - 100 * 2", 120 * S);
        assertEquals(0.0, res2.samples().get(0).value());
    }

    @Test
    void grammarGroupingAfterBody() {
        TsPromQl q = new TsPromQl(fixture());
        // grouping clause trailing the body is accepted too.
        TsPromQlResult res = q.evalInstant("sum (http_requests_total) by (job)", 120 * S);
        assertEquals(2, res.size());
    }

    private static void assertParse(String query) {
        TsPromQl q = new TsPromQl(fixture());
        TsPromQlException e = assertThrows(TsPromQlException.class, () -> q.evalInstant(query, 0));
        assertEquals(TsPromQlException.Kind.PARSE, e.kind());
    }
}
