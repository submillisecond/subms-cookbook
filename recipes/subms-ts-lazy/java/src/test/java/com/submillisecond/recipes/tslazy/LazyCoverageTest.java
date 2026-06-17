package com.submillisecond.recipes.tslazy;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * Branch coverage for the typed paths the happy-path suite does not hit: Bool
 * and Str columns through sort / take / scalar, a non-Bool filter predicate, a
 * When expr through the optimiser, and the ResultFrame accessor surface.
 */
class LazyCoverageTest {

    // A frame with a Bool flag column + a Str label column, so the Bool/Str
    // branches of take / sort / scalar are exercised.
    private static TsDataFrame mixedFrame() {
        TsSeriesD px = new TsSeriesD();
        TsSeries<Boolean> flag = new TsSeries<>();
        TsSeries<String> label = new TsSeries<>();
        double[] pxs = {3, 1, 2, 5, 4};
        boolean[] flags = {true, false, true, false, true};
        String[] labels = {"c", "a", "e", "b", "d"};
        for (int i = 0; i < 5; i++) {
            px.push(i, pxs[i]);
            flag.push(i, flags[i]);
            label.push(i, labels[i]);
        }
        return new TsDataFrame()
                .withColumn("px", new TsColumn.F64(px))
                .withColumn("flag", new TsColumn.Bool(flag))
                .withColumn("label", new TsColumn.Str(label));
    }

    @Test
    void filterOnBoolColumnAndKeepBoolColumn() {
        // select px + flag downstream so projection pushdown keeps both.
        ResultFrame r = new LazyTsFrame(mixedFrame())
                .filter(TsExpr.col("flag").eq(TsExpr.litBool(true)))
                .select("px", "flag")
                .collect();
        // flags true at i = 0, 2, 4 -> px 3, 2, 4.
        assertEquals(3, r.nrows());
        assertEquals(TsValue.ofDouble(3.0), r.cell(0, "px").orElseThrow());
        // The Bool column survives the gather/take path.
        assertEquals(TsValue.ofBool(true), r.cell(0, "flag").orElseThrow());
    }

    @Test
    void sortByStringColumnAscending() {
        ResultFrame r = new LazyTsFrame(mixedFrame())
                .sortBy("label", true)
                .select("label")
                .collect();
        assertEquals(List.of("a", "b", "c", "d", "e"),
                r.column("label").orElseThrow() instanceof
                        com.submillisecond.recipes.tsexpr.TsArray.Str s
                        ? List.of(s.values())
                        : List.of());
    }

    @Test
    void sortByBoolColumnDescending() {
        // Stable: descending bool puts the trues (px 3,2,4) before the falses.
        ResultFrame r = new LazyTsFrame(mixedFrame())
                .sortBy("flag", false)
                .select("px", "flag")
                .collect();
        assertEquals(TsValue.ofBool(true), r.cell(0, "flag").orElseThrow());
        assertEquals(TsValue.ofBool(false), r.cell(4, "flag").orElseThrow());
    }

    @Test
    void aggBoolAndStringScalars() {
        ResultFrame r = new LazyTsFrame(mixedFrame()).agg(List.of(
                new PlanNode.NamedExpr("min_label", TsExpr.col("label").min()),
                new PlanNode.NamedExpr("n_flag", TsExpr.col("flag").count())));
        assertEquals(TsValue.ofString("a"), r.cell(0, "min_label").orElseThrow());
        assertEquals(TsValue.ofLong(5L), r.cell(0, "n_flag").orElseThrow());
    }

    @Test
    void whenExprInWithColumnIsCertifiableAndOptimised() {
        // A When expr reads px + flag; projection pushdown must keep both and
        // drop label. Also walks the optimiser's When-reference branch.
        LazyTsFrame lazy = new LazyTsFrame(mixedFrame())
                .withColumn("scored",
                        TsExpr.when(
                                TsExpr.col("flag").eq(TsExpr.litBool(true)),
                                TsExpr.col("px"),
                                TsExpr.litF64(0.0)))
                .select("scored");
        String plan = lazy.explain();
        assertTrue(plan.contains("px"), plan);
        assertTrue(plan.contains("flag"), plan);
        assertFalse(plan.contains("label"), plan);
        ResultFrame r = lazy.collect();
        // scored = px where flag, else 0 -> [3, 0, 2, 0, 4].
        assertEquals(TsValue.ofDouble(3.0), r.cell(0, "scored").orElseThrow());
        assertEquals(TsValue.ofDouble(0.0), r.cell(1, "scored").orElseThrow());
        assertTrue(lazy.certify("laptop", 0).verify());
    }

    @Test
    void nonBoolPredicateThrows() {
        LazyTsFrame lazy = new LazyTsFrame(mixedFrame()).filter(TsExpr.col("px"));
        LazyException ex = assertThrows(LazyException.class, lazy::collect);
        assertEquals(LazyException.Kind.NON_BOOL_PREDICATE, ex.kind());
        assertTrue(ex.getMessage().contains("Bool"));
    }

    @Test
    void resultFrameAccessors() {
        ResultFrame r = new LazyTsFrame(mixedFrame()).select("px", "flag").collect();
        assertEquals(2, r.ncols());
        assertEquals(5, r.nrows());
        assertFalse(r.isEmpty());
        assertEquals(5, r.ts().length);
        assertTrue(r.column("missing").isEmpty());
        assertTrue(r.cell(0, "missing").isEmpty());
    }

    @Test
    void resultFrameEqualsAndRoundTrip() {
        ResultFrame a = new LazyTsFrame(mixedFrame()).select("px").collect();
        ResultFrame b = new LazyTsFrame(mixedFrame()).select("px").collect();
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        ResultFrame diff = new LazyTsFrame(mixedFrame()).select("flag").collect();
        assertNotEquals(a, diff);
        assertNotEquals(a, "not a frame");
        TsDataFrame df = a.intoDataFrame();
        assertEquals(List.of("px"), df.columnNames());
    }

    @Test
    void optimiseReturnsRewrittenFrame() {
        LazyTsFrame lazy = new LazyTsFrame(mixedFrame())
                .withColumn("g", TsExpr.col("px").add(TsExpr.litF64(1.0)))
                .filter(TsExpr.col("px").gt(TsExpr.litF64(2.0)));
        LazyTsFrame opt = lazy.optimise();
        // The optimised frame's recorded nodes start with the pushed-down filter
        // (or an inserted projection), differing from the raw record.
        assertNotEquals(lazy.nodes(), opt.nodes());
        // optimise() is idempotent: re-optimising the result is a no-op.
        assertEquals(opt.nodes(), opt.optimise().nodes());
    }
}
