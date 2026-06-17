package com.submillisecond.recipes.tslazy;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsplan.TsLatencyCertificate;
import com.submillisecond.recipes.tsplan.TsPlan;

class LazyTsFrameTest {

    private static TsDataFrame frame() {
        TsSeriesD px = new TsSeriesD();
        TsSeriesL qty = new TsSeriesL();
        TsSeries<String> side = new TsSeries<>();
        double[] pxs = {10, 11, 12, 13, 14, 15, 16, 17};
        long[] qtys = {5, 9, 2, 7, 4, 8, 1, 6};
        for (int i = 0; i < 8; i++) {
            px.push(i, pxs[i]);
            qty.push(i, qtys[i]);
            side.push(i, i % 2 == 0 ? "buy" : "sell");
        }
        return new TsDataFrame()
                .withColumn("px", new TsColumn.F64(px))
                .withColumn("qty", new TsColumn.I64(qty))
                .withColumn("side", new TsColumn.Str(side));
    }

    private static List<Double> f64Col(ResultFrame r, String name) {
        List<Double> out = new ArrayList<>();
        for (int i = 0; i < r.nrows(); i++) {
            TsValue v = r.cell(i, name).orElse(null);
            out.add(v instanceof TsValue.F64 d ? d.value() : null);
        }
        return out;
    }

    private static List<Long> i64Col(ResultFrame r, String name) {
        List<Long> out = new ArrayList<>();
        for (int i = 0; i < r.nrows(); i++) {
            TsValue v = r.cell(i, name).orElse(null);
            out.add(v instanceof TsValue.I64 l ? l.value() : null);
        }
        return out;
    }

    @Test
    void pipelineCollectMatchesReference() {
        ResultFrame result = new LazyTsFrame(frame())
                .filter(TsExpr.col("px").gt(TsExpr.litF64(12.0)))
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .select("gross", "px")
                .sortBy("gross", true)
                .limit(3)
                .collect();
        // px > 12 rows gross = 91, 56, 120, 16, 102; sorted asc: 16,56,91,102,120;
        // limit 3 -> [16, 56, 91] with px [16, 14, 13].
        assertEquals(3, result.nrows());
        assertEquals(List.of(16.0, 56.0, 91.0), f64Col(result, "gross"));
        assertEquals(List.of(16.0, 14.0, 13.0), f64Col(result, "px"));
        assertEquals(List.of("gross", "px"), result.columnNames());
    }

    @Test
    void aggTerminalWholeFrame() {
        ResultFrame result = new LazyTsFrame(frame()).agg(List.of(
                new PlanNode.NamedExpr("px_sum", TsExpr.col("px").sum()),
                new PlanNode.NamedExpr("qty_max", TsExpr.col("qty").max()),
                new PlanNode.NamedExpr("n", TsExpr.col("px").count())));
        assertEquals(1, result.nrows());
        assertEquals(List.of(108.0), f64Col(result, "px_sum"));
        assertEquals(List.of(9L), i64Col(result, "qty_max"));
        assertEquals(List.of(8L), i64Col(result, "n"));
    }

    @Test
    void optimisePreservesResults() {
        ResultFrame optimised = buildMixed().collect();
        ResultFrame raw = buildMixed().collectUnoptimised();
        assertEquals(raw, optimised);
    }

    private static LazyTsFrame buildMixed() {
        return new LazyTsFrame(frame())
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .filter(TsExpr.col("px").gt(TsExpr.litF64(11.0)))
                .withColumn("half", TsExpr.col("px").div(TsExpr.litF64(2.0)))
                .filter(TsExpr.col("qty").ge(TsExpr.litI64(4)))
                .select("gross", "half", "px")
                .sortBy("gross", false);
    }

    @Test
    void projectionPushdownDropsUnreferencedColumn() {
        String plan = new LazyTsFrame(frame())
                .filter(TsExpr.col("px").gt(TsExpr.litF64(10.0)))
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .select("gross")
                .explain();
        assertTrue(plan.contains("Select [px, qty]"), plan);
        assertFalse(plan.contains("side"), plan);
    }

    @Test
    void predicatePushdownReordersFilterAboveWithColumn() {
        LazyTsFrame lazy = new LazyTsFrame(frame())
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .filter(TsExpr.col("px").gt(TsExpr.litF64(12.0)));
        String raw = lazy.explainUnoptimised();
        String opt = lazy.explain();
        assertTrue(raw.indexOf("WithColumn") < raw.indexOf("Filter"));
        assertTrue(opt.indexOf("Filter") < opt.indexOf("WithColumn"), opt);
    }

    @Test
    void predicatePushdownBlockedWhenFilterReadsDerivedColumn() {
        String opt = new LazyTsFrame(frame())
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .filter(TsExpr.col("gross").gt(TsExpr.litF64(100.0)))
                .explain();
        assertTrue(opt.indexOf("WithColumn") < opt.indexOf("Filter"), opt);
    }

    @Test
    void filterOnStringColumn() {
        ResultFrame result = new LazyTsFrame(frame())
                .filter(TsExpr.col("side").eq(TsExpr.litStr("buy")))
                .select("px")
                .collect();
        assertEquals(4, result.nrows());
        assertEquals(List.of(10.0, 12.0, 14.0, 16.0), f64Col(result, "px"));
    }

    @Test
    void withColumnComputedExpr() {
        ResultFrame result = new LazyTsFrame(frame())
                .withColumn("bumped", TsExpr.col("px").add(TsExpr.litF64(100.0)))
                .select("bumped")
                .limit(2)
                .collect();
        assertEquals(List.of(110.0, 111.0), f64Col(result, "bumped"));
    }

    @Test
    void sortAscendingAndDescending() {
        ResultFrame asc = new LazyTsFrame(frame()).sortBy("qty", true).select("qty").collect();
        assertEquals(List.of(1L, 2L, 4L, 5L, 6L, 7L, 8L, 9L), i64Col(asc, "qty"));
        ResultFrame desc = new LazyTsFrame(frame()).sortBy("qty", false).select("qty").collect();
        assertEquals(List.of(9L, 8L, 7L, 6L, 5L, 4L, 2L, 1L), i64Col(desc, "qty"));
    }

    @Test
    void limitTruncates() {
        assertEquals(3, new LazyTsFrame(frame()).select("px").limit(3).collect().nrows());
        assertEquals(8, new LazyTsFrame(frame()).select("px").limit(999).collect().nrows());
    }

    @Test
    void emptyPipelineEqualsSource() {
        ResultFrame result = new LazyTsFrame(frame()).collect();
        assertEquals(8, result.nrows());
        assertEquals(List.of("px", "qty", "side"), result.columnNames());
        TsDataFrame df = result.intoDataFrame();
        assertEquals(List.of("px", "qty", "side"), df.columnNames());
    }

    @Test
    void explainRendersOpsInOrder() {
        String plan = new LazyTsFrame(frame())
                .filter(TsExpr.col("px").gt(TsExpr.litF64(11.0)))
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .sortBy("gross", true)
                .limit(2)
                .select("gross")
                .explainUnoptimised();
        assertTrue(plan.indexOf("Filter") < plan.indexOf("WithColumn gross"));
        assertTrue(plan.indexOf("WithColumn gross") < plan.indexOf("SortBy gross asc"));
        assertTrue(plan.indexOf("SortBy gross asc") < plan.indexOf("Limit 2"));
        assertTrue(plan.indexOf("Limit 2") < plan.indexOf("Select [gross]"));
    }

    @Test
    void certifyTotalIsSumOfNodeCostsPlusOverhead() {
        LazyTsFrame lazy = new LazyTsFrame(frame())
                .filter(TsExpr.col("px").gt(TsExpr.litF64(11.0)))
                .withColumn("gross", TsExpr.col("px").mul(TsExpr.col("qty")))
                .select("gross");
        TsPlan plan = lazy.buildPlan();
        TsLatencyCertificate cert = lazy.certify("ci-dedicated", 0);
        long expected = plan.plannerOverheadNs();
        for (var s : plan.stages()) {
            expected += s.p99Ns();
        }
        assertEquals(expected, cert.totalP99Ns());
        assertTrue(cert.verify());
        assertTrue(cert.meetsBudget(10_000_000L));
        assertEquals("ci-dedicated", cert.hardwareTier());
    }

    @Test
    void certifyNodeCostsMatchModel() {
        PlanNode select = new PlanNode.Select(List.of("a"));
        PlanNode filter = new PlanNode.Filter(TsExpr.col("a").gt(TsExpr.litF64(0.0)));
        PlanNode agg1 = new PlanNode.Agg(List.of(new PlanNode.NamedExpr("s", TsExpr.col("a").sum())));
        PlanNode agg2 = new PlanNode.Agg(List.of(
                new PlanNode.NamedExpr("s", TsExpr.col("a").sum()),
                new PlanNode.NamedExpr("m", TsExpr.col("a").mean())));
        assertEquals(2 * LazyTsFrame.nodeCostNs(agg1), LazyTsFrame.nodeCostNs(agg2));
        assertTrue(LazyTsFrame.nodeCostNs(filter) > LazyTsFrame.nodeCostNs(select));
    }

    @Test
    void emptyPlanCertifiesOverheadOnly() {
        TsLatencyCertificate cert = new LazyTsFrame(frame()).certify("laptop", 0);
        assertEquals(LazyTsFrame.PLANNER_OVERHEAD_NS, cert.totalP99Ns());
        assertTrue(cert.verify());
    }

    @Test
    void unknownSortColumnThrows() {
        LazyTsFrame lazy = new LazyTsFrame(frame()).sortBy("missing", true);
        LazyException ex = assertThrows(LazyException.class, lazy::collect);
        assertEquals(LazyException.Kind.UNKNOWN_SORT_COLUMN, ex.kind());
    }

    @Test
    void kindTagsAreStable() {
        assertEquals("select", new PlanNode.Select(List.of("a")).kind());
        assertEquals("filter", new PlanNode.Filter(TsExpr.litBool(true)).kind());
        assertEquals("with_column", new PlanNode.WithColumn("a", TsExpr.litF64(1)).kind());
        assertEquals("sort_by", new PlanNode.SortBy("a", true).kind());
        assertEquals("limit", new PlanNode.Limit(1).kind());
        assertEquals("agg", new PlanNode.Agg(List.of()).kind());
    }
}
