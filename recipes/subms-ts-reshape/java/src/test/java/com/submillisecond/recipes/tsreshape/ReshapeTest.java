package com.submillisecond.recipes.tsreshape;

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
import com.submillisecond.recipes.tsexpr.TsArray;

class ReshapeTest {

    // ---------- column builders (each row i at ts=i, fully present) ----------

    private static TsColumn strCol(String... vals) {
        TsSeries<String> s = new TsSeries<>();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.Str(s);
    }

    private static TsColumn i64Col(long... vals) {
        TsSeriesL s = new TsSeriesL();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.I64(s);
    }

    private static TsColumn f64Col(double... vals) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.F64(s);
    }

    private static TsColumn boolCol(boolean... vals) {
        TsSeries<Boolean> s = new TsSeries<>();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.Bool(s);
    }

    private static TsColumn valueCol(List<TsValue> vals) {
        TsSeries<TsValue> s = new TsSeries<>();
        for (int i = 0; i < vals.size(); i++) {
            s.push(i, vals.get(i));
        }
        return new TsColumn.Value(s);
    }

    private static List<Double> colF64(TsArray arr) {
        List<Double> out = new ArrayList<>(arr.len());
        for (int i = 0; i < arr.len(); i++) {
            Optional<TsValue> c = arr.get(i);
            if (c.isPresent() && c.get() instanceof TsValue.F64 f) {
                out.add(f.value());
            } else if (c.isPresent() && c.get() instanceof TsValue.I64 l) {
                out.add((double) l.value());
            } else {
                out.add(null);
            }
        }
        return out;
    }

    private static List<String> colStr(TsArray arr) {
        List<String> out = new ArrayList<>(arr.len());
        for (int i = 0; i < arr.len(); i++) {
            Optional<TsValue> c = arr.get(i);
            out.add(c.isPresent() && c.get() instanceof TsValue.Str s ? s.value() : null);
        }
        return out;
    }

    // A long (idx, cat-as-STRING, reading) fixture. (idx=1, cat="s2") is absent
    // on purpose so we can assert a missing pivot cell.
    private static TsDataFrame longFixture() {
        return new TsDataFrame()
                .withColumn("idx", i64Col(0, 0, 1, 1, 0))
                .withColumn("cat", strCol("s1", "s2", "s1", "s1", "s1"))
                .withColumn("reading", f64Col(10, 20, 11, 13, 30));
    }

    // ---------- pivot ----------

    @Test
    void pivotLongToWideOnStringCategoryMatchesReference() {
        TsReshapeResult out = TsReshape.pivot(longFixture(), "idx", "cat", "reading", PivotAgg.SUM);
        assertEquals(2, out.nrows());
        assertEquals(3, out.ncols());
        assertEquals(List.of("idx", "s1", "s2"), out.columnNames());

        assertEquals(List.of(0.0, 1.0), colF64(out.column("idx").orElseThrow()));
        // s1: idx0 -> 10+30 = 40 ; idx1 -> 11+13 = 24.
        assertEquals(List.of(40.0, 24.0), colF64(out.column("s1").orElseThrow()));
        // s2: idx0 -> 20 ; idx1 -> ABSENT (null), the hand-rolled missing combo.
        List<Double> s2 = colF64(out.column("s2").orElseThrow());
        assertEquals(20.0, s2.get(0));
        assertEquals(null, s2.get(1));
    }

    @Test
    void pivotEachAgg() {
        TsDataFrame f = longFixture();
        assertEquals(20.0, colF64(TsReshape.pivot(f, "idx", "cat", "reading", PivotAgg.MEAN)
                .column("s1").orElseThrow()).get(0));
        assertEquals(10.0, colF64(TsReshape.pivot(f, "idx", "cat", "reading", PivotAgg.MIN)
                .column("s1").orElseThrow()).get(0));
        assertEquals(30.0, colF64(TsReshape.pivot(f, "idx", "cat", "reading", PivotAgg.MAX)
                .column("s1").orElseThrow()).get(0));
        // idx0 s1 rows in input order: 10 (row0), 30 (row4) -> last is 30.
        assertEquals(30.0, colF64(TsReshape.pivot(f, "idx", "cat", "reading", PivotAgg.LAST)
                .column("s1").orElseThrow()).get(0));
        assertEquals(40.0, colF64(TsReshape.pivot(f, "idx", "cat", "reading", PivotAgg.SUM)
                .column("s1").orElseThrow()).get(0));
    }

    @Test
    void pivotUnknownColumnThrows() {
        TsDataFrame f = longFixture();
        assertThrows(TsReshapeException.class,
                () -> TsReshape.pivot(f, "idx", "nope", "reading", PivotAgg.SUM));
    }

    // ---------- melt (headline: the Str variable column) ----------

    @Test
    void meltWideToLongCarriesStrVariableAndValueCells() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("day", i64Col(0, 1))
                .withColumn("open", f64Col(10, 20))
                .withColumn("close", f64Col(11, 22));

        TsReshapeResult out = TsReshape.melt(f, new String[] {"day"}, new String[] {"open", "close"});
        assertEquals(4, out.nrows());
        assertEquals(List.of("day", "variable", "value"), out.columnNames());

        TsArray var = out.column("variable").orElseThrow();
        assertEquals(TsDataType.STR, var.dataType());
        assertEquals(List.of("open", "close", "open", "close"), colStr(var));

        assertEquals(List.of(0.0, 0.0, 1.0, 1.0), colF64(out.column("day").orElseThrow()));

        TsArray value = out.column("value").orElseThrow();
        assertEquals(TsDataType.F64, value.dataType());
        assertEquals(List.of(10.0, 11.0, 20.0, 22.0), colF64(value));
    }

    @Test
    void meltMixedTypeValueColsCollapseToStrValueColumn() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0, 1))
                .withColumn("name", strCol("aa", "bb"))
                .withColumn("score", f64Col(1.5, 2.0));

        TsReshapeResult out = TsReshape.melt(f, new String[] {"id"}, new String[] {"name", "score"});
        TsArray value = out.column("value").orElseThrow();
        assertEquals(TsDataType.STR, value.dataType());
        assertEquals(List.of("aa", "1.5", "bb", "2"), colStr(value));
        assertEquals(List.of("name", "score", "name", "score"),
                colStr(out.column("variable").orElseThrow()));
    }

    @Test
    void meltNoValueColsThrows() {
        TsDataFrame f = new TsDataFrame().withColumn("id", i64Col(0, 1));
        assertThrows(TsReshapeException.class, () -> TsReshape.melt(f, new String[] {"id"}, new String[0]));
    }

    @Test
    void meltUnknownValueColThrows() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0))
                .withColumn("v", f64Col(1.0));
        assertThrows(TsReshapeException.class,
                () -> TsReshape.melt(f, new String[] {"id"}, new String[] {"v", "missing"}));
    }

    // ---------- explode ----------

    @Test
    void explodeValueArrayEmitsOneRowPerElement() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0, 1))
                .withColumn("tags", valueCol(List.of(
                        new TsValue.Array(List.of(TsValue.ofDouble(1.0), TsValue.ofDouble(2.0))),
                        new TsValue.Array(List.of(TsValue.ofDouble(3.0))))));

        TsReshapeResult out = TsReshape.explode(f, "tags");
        assertEquals(3, out.nrows());
        assertEquals(List.of(0.0, 0.0, 1.0), colF64(out.column("id").orElseThrow()));
        assertEquals(List.of(1.0, 2.0, 3.0), colF64(out.column("tags").orElseThrow()));
    }

    @Test
    void explodeEmptyArrayDropsTheRow() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0, 1, 2))
                .withColumn("tags", valueCol(List.of(
                        new TsValue.Array(List.of(TsValue.ofDouble(1.0))),
                        new TsValue.Array(List.of()),
                        new TsValue.Array(List.of(TsValue.ofDouble(9.0), TsValue.ofDouble(8.0))))));

        TsReshapeResult out = TsReshape.explode(f, "tags");
        assertEquals(3, out.nrows());
        assertEquals(List.of(0.0, 2.0, 2.0), colF64(out.column("id").orElseThrow()));
        assertEquals(List.of(1.0, 9.0, 8.0), colF64(out.column("tags").orElseThrow()));
    }

    @Test
    void explodeStringElementsEmitStrColumn() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0))
                .withColumn("labels", valueCol(List.of(
                        new TsValue.Array(List.of(TsValue.ofString("x"), TsValue.ofString("y"))))));
        TsReshapeResult out = TsReshape.explode(f, "labels");
        assertEquals(2, out.nrows());
        assertEquals(List.of("x", "y"), colStr(out.column("labels").orElseThrow()));
    }

    @Test
    void explodeUnknownColumnThrows() {
        TsDataFrame f = new TsDataFrame().withColumn("id", i64Col(0));
        assertThrows(TsReshapeException.class, () -> TsReshape.explode(f, "nope"));
    }

    // ---------- vstack / hstack ----------

    @Test
    void vstackSameSchemaConcatenatesRows() {
        TsDataFrame a = new TsDataFrame()
                .withColumn("k", strCol("a", "b"))
                .withColumn("v", f64Col(1, 2));
        TsDataFrame b = new TsDataFrame()
                .withColumn("k", strCol("c"))
                .withColumn("v", f64Col(3));
        TsReshapeResult out = TsReshape.vstack(a, b);
        assertEquals(3, out.nrows());
        assertEquals(List.of("a", "b", "c"), colStr(out.column("k").orElseThrow()));
        assertEquals(List.of(1.0, 2.0, 3.0), colF64(out.column("v").orElseThrow()));
    }

    @Test
    void vstackSchemaMismatchThrows() {
        TsDataFrame a = new TsDataFrame().withColumn("k", f64Col(1));
        TsDataFrame b = new TsDataFrame().withColumn("other", f64Col(1));
        assertThrows(TsReshapeException.class, () -> TsReshape.vstack(a, b));
    }

    @Test
    void hstackWithCollisionSuffixes() {
        TsDataFrame a = new TsDataFrame()
                .withColumn("id", i64Col(0, 1))
                .withColumn("v", f64Col(1, 2));
        TsDataFrame b = new TsDataFrame()
                .withColumn("v", f64Col(3, 4))
                .withColumn("w", f64Col(5, 6));
        TsReshapeResult out = TsReshape.hstack(a, b);
        assertEquals(List.of("id", "v_a", "v_b", "w"), out.columnNames());
        assertEquals(2, out.nrows());
        assertEquals(List.of(1.0, 2.0), colF64(out.column("v_a").orElseThrow()));
        assertEquals(List.of(3.0, 4.0), colF64(out.column("v_b").orElseThrow()));
    }

    @Test
    void hstackRowCountMismatchThrows() {
        TsDataFrame a = new TsDataFrame().withColumn("a", f64Col(1, 2));
        TsDataFrame b = new TsDataFrame().withColumn("b", f64Col(3));
        assertThrows(TsReshapeException.class, () -> TsReshape.hstack(a, b));
    }

    // ---------- row set-ops ----------

    private static TsDataFrame[] pair() {
        TsDataFrame a = new TsDataFrame()
                .withColumn("k", strCol("x", "y", "x", "z"))
                .withColumn("v", i64Col(1, 2, 1, 3));
        TsDataFrame b = new TsDataFrame()
                .withColumn("k", strCol("y", "z", "w"))
                .withColumn("v", i64Col(2, 3, 4));
        return new TsDataFrame[] {a, b};
    }

    @Test
    void unionDistinctRowsInDeterministicOrder() {
        TsDataFrame[] p = pair();
        TsReshapeResult out = TsReshape.union(p[0], p[1]);
        assertEquals(4, out.nrows());
        assertEquals(List.of("x", "y", "z", "w"), colStr(out.column("k").orElseThrow()));
        assertEquals(List.of(1.0, 2.0, 3.0, 4.0), colF64(out.column("v").orElseThrow()));
    }

    @Test
    void intersectDistinctCommonRows() {
        TsDataFrame[] p = pair();
        TsReshapeResult out = TsReshape.intersect(p[0], p[1]);
        assertEquals(2, out.nrows());
        assertEquals(List.of("y", "z"), colStr(out.column("k").orElseThrow()));
    }

    @Test
    void exceptDistinctAMinusB() {
        TsDataFrame[] p = pair();
        TsReshapeResult out = TsReshape.except(p[0], p[1]);
        assertEquals(1, out.nrows());
        assertEquals(List.of("x"), colStr(out.column("k").orElseThrow()));
        assertEquals(List.of(1.0), colF64(out.column("v").orElseThrow()));
    }

    @Test
    void setOpTypedCellsStrDistinctFromNumeric() {
        TsDataFrame a = new TsDataFrame().withColumn("k", strCol("3"));
        TsDataFrame b = new TsDataFrame().withColumn("k", strCol("4"));
        TsReshapeResult out = TsReshape.union(a, b);
        assertEquals(2, out.nrows());
        assertEquals(List.of("3", "4"), colStr(out.column("k").orElseThrow()));
    }

    @Test
    void setOpSchemaMismatchThrows() {
        TsDataFrame a = new TsDataFrame().withColumn("k", f64Col(1));
        TsDataFrame b = new TsDataFrame().withColumn("j", f64Col(1));
        assertThrows(TsReshapeException.class, () -> TsReshape.union(a, b));
        assertThrows(TsReshapeException.class, () -> TsReshape.intersect(a, b));
        assertThrows(TsReshapeException.class, () -> TsReshape.except(a, b));
    }

    // ---------- empty frames + flattening helpers ----------

    @Test
    void emptyFrameReshapesToEmpty() {
        TsDataFrame empty = new TsDataFrame();
        assertThrows(TsReshapeException.class, () -> TsReshape.pivot(empty, "i", "c", "v", PivotAgg.SUM));
        TsReshapeResult out = TsReshape.vstack(empty, new TsDataFrame());
        assertTrue(out.isEmpty());
        assertEquals(0, out.ncols());
    }

    @Test
    void pivotEmptyAfterAllRowsSkipped() {
        // every value cell is a Bool, which cannot reduce to f64 -> all skipped.
        TsDataFrame f = new TsDataFrame()
                .withColumn("idx", i64Col(0, 1))
                .withColumn("cat", strCol("a", "b"))
                .withColumn("val", boolCol(true, false));
        TsReshapeResult out = TsReshape.pivot(f, "idx", "cat", "val", PivotAgg.SUM);
        assertTrue(out.isEmpty());
    }

    @Test
    void frameColumnsExposesTypedFlattening() {
        TsReshape.FrameColumns fc = TsReshape.frameColumns(longFixture());
        assertEquals(3, fc.columns().size());
        assertEquals("idx", fc.names().get(0));
        assertEquals(TsDataType.I64, fc.columns().get(0).dataType());
        assertEquals(TsDataType.STR, fc.columns().get(1).dataType());
    }

    @Test
    void frameValueCellsExposesValueColumns() {
        TsDataFrame f = new TsDataFrame()
                .withColumn("id", i64Col(0))
                .withColumn("doc", valueCol(List.of(
                        new TsValue.Array(List.of(TsValue.ofDouble(1.0))))));
        var vc = TsReshape.frameValueCells(f);
        assertTrue(vc.containsKey("doc"));
        assertFalse(vc.containsKey("id"));
        assertEquals(1, vc.get("doc").size());
    }

    @Test
    void resultColumnAtAndNames() {
        TsReshapeResult out = TsReshape.pivot(longFixture(), "idx", "cat", "reading", PivotAgg.SUM);
        assertTrue(out.columnAt(0).isPresent());
        assertTrue(out.columnAt(99).isEmpty());
        assertTrue(out.column("nope").isEmpty());
        assertEquals(out.ncols(), out.columnNames().size());
    }
}
