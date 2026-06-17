package com.submillisecond.recipes.tsexpr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;

/**
 * Exercises the typed {@link TsArray} surface directly (get / fillNull /
 * dropNulls / validCount / equals / hashCode across every variant) and the
 * eval branches the primary behavioural test does not reach (typed literals,
 * when / agg over each element type, bool compares, length-mismatch guards).
 */
class TsArrayAndEvalTest {

    private static TsColumn i64Col(long[] ts, long[] vs) {
        TsSeriesL s = new TsSeriesL();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], vs[i]);
        return new TsColumn.I64(s);
    }

    private static TsColumn boolCol(long[] ts, boolean[] vs) {
        TsSeries<Boolean> s = new TsSeries<>();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], vs[i]);
        return new TsColumn.Bool(s);
    }

    private static TsColumn strCol(long[] ts, String[] vs) {
        TsSeries<String> s = new TsSeries<>();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], vs[i]);
        return new TsColumn.Str(s);
    }

    private static List<Optional<TsValue>> pairs(TsArray a) {
        List<Optional<TsValue>> out = new ArrayList<>(a.len());
        for (int i = 0; i < a.len(); i++) out.add(a.get(i));
        return out;
    }

    @Test
    void f64ArraySurface() {
        TsArray a = new TsArray.F64(new double[] {1.0, 2.0, 3.0}, new boolean[] {true, false, true});
        assertEquals(TsDataType.F64, a.dataType());
        assertEquals(3, a.len());
        assertFalse(a.isEmpty());
        assertEquals(2, a.validCount());
        assertEquals(Optional.of(TsValue.ofDouble(1.0)), a.get(0));
        assertEquals(Optional.empty(), a.get(1));

        TsArray filled = a.fillNull(TsValue.ofDouble(-9.0));
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofDouble(1.0)),
                        Optional.of(TsValue.ofDouble(-9.0)),
                        Optional.of(TsValue.ofDouble(3.0))),
                pairs(filled));
        // an i64 fill widens into the f64 array.
        TsArray filledI = a.fillNull(TsValue.ofLong(7));
        assertEquals(Optional.of(TsValue.ofDouble(7.0)), filledI.get(1));

        TsArray dropped = a.dropNulls();
        assertEquals(2, dropped.len());
        assertEquals(
                List.of(Optional.of(TsValue.ofDouble(1.0)), Optional.of(TsValue.ofDouble(3.0))),
                pairs(dropped));

        assertEquals(
                new TsArray.F64(new double[] {1.0, 2.0, 3.0}, new boolean[] {true, false, true}), a);
        assertNotEquals(
                new TsArray.F64(new double[] {1.0, 2.0, 9.0}, new boolean[] {true, false, true}), a);
        assertEquals(
                new TsArray.F64(new double[] {1.0, 2.0, 3.0}, new boolean[] {true, false, true})
                        .hashCode(),
                a.hashCode());
        assertThrows(
                IllegalArgumentException.class,
                () -> new TsArray.F64(new double[] {1.0}, new boolean[] {true, false}));
    }

    @Test
    void i64ArraySurface() {
        TsArray a = new TsArray.I64(new long[] {5, 6, 7}, new boolean[] {true, false, true});
        assertEquals(TsDataType.I64, a.dataType());
        assertEquals(Optional.of(TsValue.ofLong(5)), a.get(0));
        assertEquals(Optional.empty(), a.get(1));
        assertEquals(2, a.validCount());

        TsArray filled = a.fillNull(TsValue.ofLong(-1));
        assertEquals(Optional.of(TsValue.ofLong(-1)), filled.get(1));
        // a non-i64 fill falls back to 0.
        assertEquals(Optional.of(TsValue.ofLong(0)), a.fillNull(TsValue.ofDouble(2.0)).get(1));

        TsArray dropped = a.dropNulls();
        assertEquals(2, dropped.len());
        assertEquals(Optional.of(TsValue.ofLong(7)), dropped.get(1));

        assertEquals(new TsArray.I64(new long[] {5, 6, 7}, new boolean[] {true, false, true}), a);
        assertNotEquals("not an array", a);
        assertEquals(
                new TsArray.I64(new long[] {5, 6, 7}, new boolean[] {true, false, true}).hashCode(),
                a.hashCode());
        assertThrows(
                IllegalArgumentException.class,
                () -> new TsArray.I64(new long[] {1}, new boolean[] {true, false}));
    }

    @Test
    void boolArraySurface() {
        TsArray a = new TsArray.Bool(new boolean[] {true, false, true}, new boolean[] {true, false, true});
        assertEquals(TsDataType.BOOL, a.dataType());
        assertEquals(Optional.of(TsValue.ofBool(true)), a.get(0));
        assertEquals(Optional.empty(), a.get(1));

        TsArray filled = a.fillNull(TsValue.ofBool(true));
        assertEquals(Optional.of(TsValue.ofBool(true)), filled.get(1));
        // a non-bool fill falls back to false.
        assertEquals(Optional.of(TsValue.ofBool(false)), a.fillNull(TsValue.ofLong(1)).get(1));

        TsArray dropped = a.dropNulls();
        assertEquals(2, dropped.len());
        assertEquals(Optional.of(TsValue.ofBool(true)), dropped.get(1));

        assertEquals(
                new TsArray.Bool(new boolean[] {true, false, true}, new boolean[] {true, false, true}),
                a);
        assertNotEquals(
                new TsArray.Bool(new boolean[] {false, false, true}, new boolean[] {true, false, true}),
                a);
        assertEquals(
                new TsArray.Bool(new boolean[] {true, false, true}, new boolean[] {true, false, true})
                        .hashCode(),
                a.hashCode());
        assertThrows(
                IllegalArgumentException.class,
                () -> new TsArray.Bool(new boolean[] {true}, new boolean[] {true, false}));
    }

    @Test
    void strArraySurface() {
        TsArray a = new TsArray.Str(new String[] {"a", "b", "c"}, new boolean[] {true, false, true});
        assertEquals(TsDataType.STR, a.dataType());
        assertEquals(Optional.of(TsValue.ofString("a")), a.get(0));
        assertEquals(Optional.empty(), a.get(1));

        TsArray filled = a.fillNull(TsValue.ofString("z"));
        assertEquals(Optional.of(TsValue.ofString("z")), filled.get(1));
        // a non-str fill falls back to empty string.
        assertEquals(Optional.of(TsValue.ofString("")), a.fillNull(TsValue.ofLong(1)).get(1));

        TsArray dropped = a.dropNulls();
        assertEquals(2, dropped.len());
        assertEquals(Optional.of(TsValue.ofString("c")), dropped.get(1));

        assertEquals(
                new TsArray.Str(new String[] {"a", "b", "c"}, new boolean[] {true, false, true}), a);
        assertNotEquals(
                new TsArray.Str(new String[] {"a", "x", "c"}, new boolean[] {true, true, true}), a);
        assertEquals(
                new TsArray.Str(new String[] {"a", "b", "c"}, new boolean[] {true, false, true})
                        .hashCode(),
                a.hashCode());
        assertThrows(
                IllegalArgumentException.class,
                () -> new TsArray.Str(new String[] {"a"}, new boolean[] {true, false}));
    }

    @Test
    void typedLiteralsBroadcast() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", i64Col(new long[] {0, 1}, new long[] {1, 2}));
        assertEquals(
                List.of(Optional.of(TsValue.ofLong(9)), Optional.of(TsValue.ofLong(9))),
                pairs(Eval.eval(TsExpr.litI64(9), frame)));
        assertEquals(
                List.of(Optional.of(TsValue.ofBool(true)), Optional.of(TsValue.ofBool(true))),
                pairs(Eval.eval(TsExpr.litBool(true), frame)));
        assertEquals(
                List.of(Optional.of(TsValue.ofString("k")), Optional.of(TsValue.ofString("k"))),
                pairs(Eval.eval(TsExpr.litStr("k"), frame)));
        assertEquals(
                List.of(Optional.of(TsValue.ofDouble(2.5)), Optional.of(TsValue.ofDouble(2.5))),
                pairs(Eval.eval(TsExpr.litF64(2.5), frame)));
    }

    @Test
    void unaryAndAggOverI64() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", i64Col(new long[] {0, 1, 2}, new long[] {-3, 4, -5}));
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofLong(3)),
                        Optional.of(TsValue.ofLong(-4)),
                        Optional.of(TsValue.ofLong(5))),
                pairs(Eval.eval(TsExpr.col("a").neg(), frame)));
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofLong(3)),
                        Optional.of(TsValue.ofLong(4)),
                        Optional.of(TsValue.ofLong(5))),
                pairs(Eval.eval(TsExpr.col("a").abs(), frame)));
        // sum / mean of an i64 column promote to f64.
        assertEquals(TsValue.ofDouble(-4.0), Eval.evalScalar(TsExpr.col("a").sum(), frame));
        assertEquals(TsValue.ofDouble(-4.0 / 3.0), Eval.evalScalar(TsExpr.col("a").mean(), frame));
    }

    @Test
    void whenOverI64BoolStr() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("flag", boolCol(new long[] {0, 1}, new boolean[] {true, false}))
                .withColumn("hi", i64Col(new long[] {0, 1}, new long[] {10, 11}))
                .withColumn("lo", i64Col(new long[] {0, 1}, new long[] {20, 21}));
        TsArray pickI = Eval.eval(
                TsExpr.when(TsExpr.col("flag"), TsExpr.col("hi"), TsExpr.col("lo")), frame);
        assertEquals(
                List.of(Optional.of(TsValue.ofLong(10)), Optional.of(TsValue.ofLong(21))),
                pairs(pickI));

        TsArray pickBool = Eval.eval(
                TsExpr.when(TsExpr.col("flag"), TsExpr.litBool(true), TsExpr.litBool(false)), frame);
        assertEquals(
                List.of(Optional.of(TsValue.ofBool(true)), Optional.of(TsValue.ofBool(false))),
                pairs(pickBool));

        TsArray pickStr = Eval.eval(
                TsExpr.when(TsExpr.col("flag"), TsExpr.litStr("yes"), TsExpr.litStr("no")), frame);
        assertEquals(
                List.of(Optional.of(TsValue.ofString("yes")), Optional.of(TsValue.ofString("no"))),
                pairs(pickStr));
    }

    @Test
    void boolCompareEqNeAndUnsupportedOrd() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", boolCol(new long[] {0, 1}, new boolean[] {true, false}))
                .withColumn("b", boolCol(new long[] {0, 1}, new boolean[] {true, true}));
        assertEquals(
                List.of(Optional.of(TsValue.ofBool(true)), Optional.of(TsValue.ofBool(false))),
                pairs(Eval.eval(TsExpr.col("a").eq(TsExpr.col("b")), frame)));
        assertEquals(
                List.of(Optional.of(TsValue.ofBool(false)), Optional.of(TsValue.ofBool(true))),
                pairs(Eval.eval(TsExpr.col("a").ne(TsExpr.col("b")), frame)));
        // lt over bools is a type error.
        TsExprException ex = assertThrows(
                TsExprException.class,
                () -> Eval.eval(TsExpr.col("a").lt(TsExpr.col("b")), frame));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex.kind());
    }

    @Test
    void compareTypeMismatchAcrossKinds() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("d", i64Col(new long[] {0}, new long[] {1}))
                .withColumn("s", strCol(new long[] {0}, new String[] {"x"}));
        TsExprException ex = assertThrows(
                TsExprException.class,
                () -> Eval.eval(TsExpr.col("d").lt(TsExpr.col("s")), frame));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex.kind());
    }

    @Test
    void minMaxOverBoolIsTypeError() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", boolCol(new long[] {0, 1}, new boolean[] {true, false}));
        TsExprException ex = assertThrows(
                TsExprException.class, () -> Eval.evalScalar(TsExpr.col("a").min(), frame));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex.kind());
        // sum over a str column is also a type error.
        TsDataFrame strs = new TsDataFrame()
                .withColumn("a", strCol(new long[] {0}, new String[] {"x"}));
        TsExprException ex2 = assertThrows(
                TsExprException.class, () -> Eval.evalScalar(TsExpr.col("a").sum(), strs));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex2.kind());
    }

    @Test
    void strBranchWhenWithNullCarriesEmpty() {
        // a Str when where the cond has a gap -> that row is null, value "".
        TsDataFrame frame = new TsDataFrame()
                .withColumn("flag", boolCol(new long[] {0, 2}, new boolean[] {true, false}))
                .withColumn("a", strCol(new long[] {0, 1, 2}, new String[] {"p", "q", "r"}));
        TsArray out = Eval.eval(
                TsExpr.when(TsExpr.col("flag"), TsExpr.litStr("hi"), TsExpr.litStr("lo")), frame);
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofString("hi")),
                        Optional.empty(),
                        Optional.of(TsValue.ofString("lo"))),
                pairs(out));
    }

    @Test
    void valueColumnReadsAsF64() {
        // a VALUE column of doubles is projected onto an F64 array.
        TsSeries<TsValue> s = new TsSeries<>();
        s.push(0, TsValue.ofDouble(1.5));
        s.push(1, TsValue.ofLong(2));
        TsDataFrame frame = new TsDataFrame().withColumn("v", new TsColumn.Value(s));
        TsArray a = Eval.eval(TsExpr.col("v"), frame);
        assertEquals(TsDataType.F64, a.dataType());
        assertEquals(
                List.of(Optional.of(TsValue.ofDouble(1.5)), Optional.of(TsValue.ofDouble(2.0))),
                pairs(a));
    }

    @Test
    void f64SeriesDirectColumnRoundTrips() {
        TsSeriesD s = new TsSeriesD();
        s.push(0, 4.0);
        s.push(1, 8.0);
        TsDataFrame frame = new TsDataFrame().withColumn("a", new TsColumn.F64(s));
        assertEquals(TsValue.ofDouble(8.0), Eval.evalScalar(TsExpr.col("a").max(), frame));
        assertTrue(Eval.eval(TsExpr.col("a"), frame) instanceof TsArray.F64);
    }
}
