package com.submillisecond.recipes.tsexpr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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

class TsExprTest {

    private static TsColumn f64Col(long[] ts, double[] vs) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < ts.length; i++) s.push(ts[i], vs[i]);
        return new TsColumn.F64(s);
    }

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

    private static Optional<TsValue> f(double v) {
        return Optional.of(TsValue.ofDouble(v));
    }

    @Test
    void colPullsEachColumnType() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("d", f64Col(new long[] {0, 1}, new double[] {1.5, 2.5}))
                .withColumn("i", i64Col(new long[] {0, 1}, new long[] {7, 8}))
                .withColumn("b", boolCol(new long[] {0, 1}, new boolean[] {true, false}))
                .withColumn("s", strCol(new long[] {0, 1}, new String[] {"x", "y"}));

        TsArray d = Eval.eval(TsExpr.col("d"), frame);
        assertEquals(TsDataType.F64, d.dataType());
        assertEquals(List.of(f(1.5), f(2.5)), pairs(d));

        TsArray i = Eval.eval(TsExpr.col("i"), frame);
        assertEquals(TsDataType.I64, i.dataType());
        assertEquals(List.of(Optional.of(TsValue.ofLong(7)), Optional.of(TsValue.ofLong(8))), pairs(i));

        TsArray b = Eval.eval(TsExpr.col("b"), frame);
        assertEquals(TsDataType.BOOL, b.dataType());
        assertEquals(
                List.of(Optional.of(TsValue.ofBool(true)), Optional.of(TsValue.ofBool(false))),
                pairs(b));

        TsArray s = Eval.eval(TsExpr.col("s"), frame);
        assertEquals(TsDataType.STR, s.dataType());
        assertEquals(
                List.of(Optional.of(TsValue.ofString("x")), Optional.of(TsValue.ofString("y"))),
                pairs(s));
    }

    @Test
    void arithmeticF64AndI64StayTyped() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 1}, new double[] {2.0, 3.0}))
                .withColumn("b", f64Col(new long[] {0, 1}, new double[] {5.0, 7.0}));
        TsArray add = Eval.eval(TsExpr.col("a").add(TsExpr.col("b")), f);
        assertEquals(TsDataType.F64, add.dataType());
        assertEquals(List.of(f(7.0), f(10.0)), pairs(add));

        TsDataFrame ints = new TsDataFrame()
                .withColumn("a", i64Col(new long[] {0, 1}, new long[] {10, 4}))
                .withColumn("b", i64Col(new long[] {0, 1}, new long[] {3, 6}));
        TsArray mul = Eval.eval(TsExpr.col("a").mul(TsExpr.col("b")), ints);
        assertEquals(TsDataType.I64, mul.dataType());
        assertEquals(
                List.of(Optional.of(TsValue.ofLong(30)), Optional.of(TsValue.ofLong(24))),
                pairs(mul));
    }

    @Test
    void arithmeticMixedPromotesToF64() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("d", f64Col(new long[] {0, 1}, new double[] {2.5, 4.0}))
                .withColumn("i", i64Col(new long[] {0, 1}, new long[] {2, 3}));
        TsArray r = Eval.eval(TsExpr.col("d").add(TsExpr.col("i")), frame);
        assertEquals(TsDataType.F64, r.dataType());
        assertEquals(List.of(f(4.5), f(7.0)), pairs(r));

        TsArray r2 = Eval.eval(TsExpr.col("i").mul(TsExpr.litF64(1.5)), frame);
        assertEquals(TsDataType.F64, r2.dataType());
        assertEquals(List.of(f(3.0), f(4.5)), pairs(r2));
    }

    @Test
    void divByZeroIsNullF64AndI64() {
        TsDataFrame df = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 1}, new double[] {10.0, 6.0}))
                .withColumn("b", f64Col(new long[] {0, 1}, new double[] {2.0, 0.0}));
        TsArray c = Eval.eval(TsExpr.col("a").div(TsExpr.col("b")), df);
        assertEquals(List.of(f(5.0), Optional.empty()), pairs(c));

        TsDataFrame di = new TsDataFrame()
                .withColumn("a", i64Col(new long[] {0, 1}, new long[] {10, 6}))
                .withColumn("b", i64Col(new long[] {0, 1}, new long[] {2, 0}));
        TsArray ci = Eval.eval(TsExpr.col("a").div(TsExpr.col("b")), di);
        assertEquals(List.of(Optional.of(TsValue.ofLong(5)), Optional.empty()), pairs(ci));
    }

    @Test
    void compareNumericWithPromotionYieldsBool() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 1, 2}, new double[] {1.0, 5.0, 5.0}))
                .withColumn("i", i64Col(new long[] {0, 1, 2}, new long[] {2, 4, 5}));
        TsArray gt = Eval.eval(TsExpr.col("a").gt(TsExpr.col("i")), frame);
        assertEquals(TsDataType.BOOL, gt.dataType());
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofBool(false)),
                        Optional.of(TsValue.ofBool(true)),
                        Optional.of(TsValue.ofBool(false))),
                pairs(gt));
        TsArray ge = Eval.eval(TsExpr.col("a").ge(TsExpr.col("i")), frame);
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofBool(false)),
                        Optional.of(TsValue.ofBool(true)),
                        Optional.of(TsValue.ofBool(true))),
                pairs(ge));
    }

    @Test
    void compareStrEqAndOrd() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", strCol(new long[] {0, 1, 2}, new String[] {"abc", "xyz", "m"}))
                .withColumn("b", strCol(new long[] {0, 1, 2}, new String[] {"abc", "abc", "z"}));
        TsArray eqm = Eval.eval(TsExpr.col("a").eq(TsExpr.col("b")), frame);
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofBool(true)),
                        Optional.of(TsValue.ofBool(false)),
                        Optional.of(TsValue.ofBool(false))),
                pairs(eqm));
        TsArray lt = Eval.eval(TsExpr.col("a").lt(TsExpr.col("b")), frame);
        assertEquals(
                List.of(
                        Optional.of(TsValue.ofBool(false)),
                        Optional.of(TsValue.ofBool(false)),
                        Optional.of(TsValue.ofBool(true))),
                pairs(lt));
    }

    @Test
    void whenSelectsElementwiseKeepingType() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 1}, new double[] {1.0, 9.0}))
                .withColumn("b", f64Col(new long[] {0, 1}, new double[] {2.0, 3.0}));
        TsExpr e = TsExpr.when(
                TsExpr.col("a").gt(TsExpr.col("b")),
                TsExpr.col("a"),
                TsExpr.col("b"));
        TsArray c = Eval.eval(e, frame);
        assertEquals(TsDataType.F64, c.dataType());
        assertEquals(List.of(f(2.0), f(9.0)), pairs(c));
    }

    @Test
    void aggSumMeanMinMaxCountOverValidWithGap() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 2, 3}, new double[] {1.0, 3.0, 4.0}))
                .withColumn("b", f64Col(new long[] {0, 1, 2, 3}, new double[] {9, 8, 7, 6}));
        assertEquals(TsValue.ofDouble(8.0), Eval.evalScalar(TsExpr.col("a").sum(), frame));
        assertEquals(TsValue.ofDouble(8.0 / 3.0), Eval.evalScalar(TsExpr.col("a").mean(), frame));
        assertEquals(TsValue.ofDouble(1.0), Eval.evalScalar(TsExpr.col("a").min(), frame));
        assertEquals(TsValue.ofDouble(4.0), Eval.evalScalar(TsExpr.col("a").max(), frame));
        assertEquals(TsValue.ofLong(3), Eval.evalScalar(TsExpr.col("a").count(), frame));
    }

    @Test
    void aggMinMaxKeepOperandType() {
        TsDataFrame ints = new TsDataFrame()
                .withColumn("a", i64Col(new long[] {0, 1, 2}, new long[] {5, 2, 9}));
        assertEquals(TsValue.ofLong(2), Eval.evalScalar(TsExpr.col("a").min(), ints));
        assertEquals(TsValue.ofLong(9), Eval.evalScalar(TsExpr.col("a").max(), ints));

        TsDataFrame strs = new TsDataFrame()
                .withColumn("a", strCol(new long[] {0, 1, 2}, new String[] {"b", "a", "c"}));
        assertEquals(TsValue.ofString("a"), Eval.evalScalar(TsExpr.col("a").min(), strs));
        assertEquals(TsValue.ofString("c"), Eval.evalScalar(TsExpr.col("a").max(), strs));
    }

    @Test
    void nullPropagatesThroughBinary() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 2}, new double[] {1.0, 3.0}))
                .withColumn("b", f64Col(new long[] {0, 1, 2}, new double[] {9.0, 8.0, 7.0}));
        TsArray c = Eval.eval(TsExpr.col("a").add(TsExpr.col("b")), frame);
        assertEquals(List.of(f(10.0), Optional.empty(), f(10.0)), pairs(c));
    }

    @Test
    void fillNullAndDropNulls() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 2}, new double[] {1.0, 3.0}))
                .withColumn("b", f64Col(new long[] {0, 1, 2}, new double[] {9.0, 8.0, 7.0}));
        TsArray c = Eval.eval(TsExpr.col("a"), frame);
        TsArray filled = c.fillNull(TsValue.ofDouble(-1.0));
        assertEquals(List.of(f(1.0), f(-1.0), f(3.0)), pairs(filled));
        for (boolean v : filled.valid()) assertTrue(v);

        TsArray dropped = c.dropNulls();
        assertEquals(2, dropped.len());
        assertEquals(List.of(f(1.0), f(3.0)), pairs(dropped));
    }

    @Test
    void unknownColumnThrows() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0}, new double[] {1.0}));
        TsExprException ex = assertThrows(
                TsExprException.class, () -> Eval.eval(TsExpr.col("nope"), frame));
        assertEquals(TsExprException.Kind.UNKNOWN_COLUMN, ex.kind());
    }

    @Test
    void typeMismatchThrows() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("s", strCol(new long[] {0}, new String[] {"x"}))
                .withColumn("d", f64Col(new long[] {0}, new double[] {1.0}));
        TsExprException ex = assertThrows(
                TsExprException.class,
                () -> Eval.eval(TsExpr.col("s").add(TsExpr.col("d")), frame));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex.kind());

        TsExprException ex2 = assertThrows(
                TsExprException.class,
                () -> Eval.eval(
                        TsExpr.when(
                                TsExpr.col("d").gt(TsExpr.litF64(0.0)),
                                TsExpr.col("d"),
                                TsExpr.litStr("nope")),
                        frame));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex2.kind());

        TsExprException ex3 = assertThrows(
                TsExprException.class, () -> Eval.evalScalar(TsExpr.col("d"), frame));
        assertEquals(TsExprException.Kind.NOT_SCALAR, ex3.kind());
    }

    @Test
    void unaryNegAbsNumericOnly() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 1}, new double[] {-3.0, 4.0}));
        assertEquals(List.of(f(3.0), f(-4.0)), pairs(Eval.eval(TsExpr.col("a").neg(), frame)));
        assertEquals(List.of(f(3.0), f(4.0)), pairs(Eval.eval(TsExpr.col("a").abs(), frame)));

        TsDataFrame strs = new TsDataFrame()
                .withColumn("a", strCol(new long[] {0}, new String[] {"x"}));
        TsExprException ex = assertThrows(
                TsExprException.class, () -> Eval.eval(TsExpr.col("a").neg(), strs));
        assertEquals(TsExprException.Kind.TYPE_MISMATCH, ex.kind());
    }

    @Test
    void deepNestedExpressionMatchesHandComputed() {
        long[] openTs = {0, 1, 2, 3};
        double[] openV = {5.0, 2.0, 8.0, 4.0};
        long[] closeTs = {0, 2, 3}; // gap at ts=1
        double[] closeV = {3.0, 1.0, 9.0};
        TsDataFrame frame = new TsDataFrame()
                .withColumn("open", f64Col(openTs, openV))
                .withColumn("close", f64Col(closeTs, closeV));
        TsExpr e = TsExpr.when(
                TsExpr.col("close").gt(TsExpr.col("open")),
                TsExpr.col("close").sub(TsExpr.col("open")).mul(TsExpr.litF64(2.0)),
                TsExpr.litF64(0.0))
                .mean();

        // union axis {0,1,2,3}; ts=1 has no close -> cond null -> excluded.
        double[][] rows = {{5.0, 3.0}, {8.0, 1.0}, {4.0, 9.0}};
        double sum = 0.0;
        int cnt = 0;
        for (double[] row : rows) {
            double o = row[0];
            double cl = row[1];
            sum += cl > o ? (cl - o) * 2.0 : 0.0;
            cnt++;
        }
        double want = sum / cnt;
        assertEquals(TsValue.ofDouble(want), Eval.evalScalar(e, frame));
    }

    @Test
    void whenWithNullCondIsNull() {
        TsDataFrame frame = new TsDataFrame()
                .withColumn("a", f64Col(new long[] {0, 2}, new double[] {1.0, 1.0}))
                .withColumn("guard", boolCol(new long[] {0, 1, 2}, new boolean[] {true, true, true}));
        TsExpr e = TsExpr.when(
                TsExpr.col("a").gt(TsExpr.litF64(0.0)),
                TsExpr.litF64(10.0),
                TsExpr.litF64(20.0));
        TsArray c = Eval.eval(e, frame);
        assertEquals(List.of(f(10.0), Optional.empty(), f(10.0)), pairs(c));
    }

    @Test
    void emptyFrameAggsHaveDefinedScalars() {
        TsDataFrame frame = new TsDataFrame().withColumn("a", new TsColumn.F64(new TsSeriesD()));
        assertEquals(TsValue.ofLong(0L), Eval.evalScalar(TsExpr.col("a").count(), frame));
        assertEquals(TsValue.ofDouble(0.0), Eval.evalScalar(TsExpr.col("a").sum(), frame));
        TsValue mean = Eval.evalScalar(TsExpr.col("a").mean(), frame);
        assertTrue(mean instanceof TsValue.F64 m && Double.isNaN(m.value()));
        assertEquals(TsValue.nullValue(), Eval.evalScalar(TsExpr.col("a").min(), frame));
        assertFalse(frame.isEmpty());
    }
}
