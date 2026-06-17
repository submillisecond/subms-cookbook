package com.submillisecond.recipes.tswindow;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Arrays;
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
import com.submillisecond.recipes.tsexpr.TsExpr;

class WindowTest {

    // ---------- column builders + frame ----------

    private static TsColumn f64Col(long[] ts, double[] vals) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < ts.length; i++) {
            s.push(ts[i], vals[i]);
        }
        return new TsColumn.F64(s);
    }

    private static TsColumn i64Col(long[] ts, long[] vals) {
        TsSeriesL s = new TsSeriesL();
        for (int i = 0; i < ts.length; i++) {
            s.push(ts[i], vals[i]);
        }
        return new TsColumn.I64(s);
    }

    private static TsColumn strCol(long[] ts, String[] vals) {
        TsSeries<String> s = new TsSeries<>();
        for (int i = 0; i < ts.length; i++) {
            s.push(ts[i], vals[i]);
        }
        return new TsColumn.Str(s);
    }

    private static TsDataFrame frame(Object... cols) {
        TsDataFrame f = new TsDataFrame();
        for (int i = 0; i < cols.length; i += 2) {
            f.pushColumn((String) cols[i], (TsColumn) cols[i + 1]);
        }
        return f;
    }

    private static long[] ts(long... t) {
        return t;
    }

    private static double[] d(double... v) {
        return v;
    }

    private static long[] l(long... v) {
        return v;
    }

    private static String[] str(String... v) {
        return v;
    }

    private static TsColumn boolCol(long[] ts, boolean[] vals) {
        TsSeries<Boolean> s = new TsSeries<>();
        for (int i = 0; i < ts.length; i++) {
            s.push(ts[i], vals[i]);
        }
        return new TsColumn.Bool(s);
    }

    private static boolean[] b(boolean... v) {
        return v;
    }

    // Read an array's cells as Double options (null where null), for numeric asserts.
    private static List<Double> f64s(TsArray a) {
        List<Double> out = new ArrayList<>(a.len());
        for (int i = 0; i < a.len(); i++) {
            Optional<TsValue> c = a.get(i);
            if (c.isPresent() && c.get() instanceof TsValue.F64 x) {
                out.add(x.value());
            } else if (c.isPresent() && c.get() instanceof TsValue.I64 x) {
                out.add((double) x.value());
            } else {
                out.add(null);
            }
        }
        return out;
    }

    // Read an I64 array's cells as Long options.
    private static List<Long> i64s(TsArray a) {
        List<Long> out = new ArrayList<>(a.len());
        for (int i = 0; i < a.len(); i++) {
            Optional<TsValue> c = a.get(i);
            if (c.isPresent() && c.get() instanceof TsValue.I64 x) {
                out.add(x.value());
            } else {
                out.add(null);
            }
        }
        return out;
    }

    private static List<Double> dl(Double... v) {
        return Arrays.asList(v);
    }

    private static List<Long> ll(Long... v) {
        return Arrays.asList(v);
    }

    private static final String[] BY_KEY = {"key"};
    private static final String[] BY_SYM = {"sym"};

    // f64-keyed two-partition frame: key 1.0 at even ts, 2.0 at odd ts; val == ts.
    private static TsDataFrame twoPartitionFrame() {
        return frame(
                "key", f64Col(ts(0, 1, 2, 3, 4, 5), d(1, 2, 1, 2, 1, 2)),
                "val", f64Col(ts(0, 1, 2, 3, 4, 5), d(0, 1, 2, 3, 4, 5)));
    }

    // STRING-keyed two-partition frame: AAPL at even ts, MSFT at odd ts; px == ts.
    private static TsDataFrame twoSymbolFrame() {
        return frame(
                "sym", strCol(ts(0, 1, 2, 3, 4, 5),
                        str("AAPL", "MSFT", "AAPL", "MSFT", "AAPL", "MSFT")),
                "px", f64Col(ts(0, 1, 2, 3, 4, 5), d(0, 1, 2, 3, 4, 5)));
    }

    // ---------- tests ----------

    @Test
    void lagShiftsWithinPartitionWithNullHead() {
        TsArray got = TsWindow.lag(twoPartitionFrame(), "val", 1, BY_KEY);
        // partition 1.0 rows ts 0,2,4 -> vals 0,2,4; lag1 -> null,0,2
        // partition 2.0 rows ts 1,3,5 -> vals 1,3,5; lag1 -> null,1,3
        assertEquals(dl(null, null, 0.0, 1.0, 2.0, 3.0), f64s(got));
    }

    @Test
    void leadShiftsWithinPartitionWithNullTail() {
        TsArray got = TsWindow.lead(twoPartitionFrame(), "val", 1, BY_KEY);
        // partition 1.0: vals 0,2,4 ; lead1 -> 2,4,null  (rows 0,2,4)
        // partition 2.0: vals 1,3,5 ; lead1 -> 3,5,null  (rows 1,3,5)
        assertEquals(dl(2.0, 3.0, 4.0, 5.0, null, null), f64s(got));
    }

    @Test
    void lagStaysWithinTwoStringPartitions() {
        // The headline: typed STRING partition keys. If lag crossed partitions,
        // the row at ts=2 (AAPL) lag1 would pick ts=1's px (MSFT, 1.0); it must
        // instead pick AAPL's previous, ts=0 (0.0). Symmetric for MSFT.
        TsArray got = TsWindow.lag(twoSymbolFrame(), "px", 1, BY_SYM);
        assertEquals(TsDataType.F64, got.dataType());
        assertEquals(dl(null, null, 0.0, 1.0, 2.0, 3.0), f64s(got));
    }

    @Test
    void lagPreservesStringColumnType() {
        // lag over a non-numeric column produces a same-typed array.
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("AAPL", "AAPL", "AAPL")),
                "note", strCol(ts(0, 1, 2), str("a", "b", "c")));
        TsArray got = TsWindow.lag(f, "note", 1, BY_SYM);
        assertEquals(TsDataType.STR, got.dataType());
        assertTrue(got.get(0).isEmpty());
        assertEquals("a", ((TsValue.Str) got.get(1).orElseThrow()).value());
        assertEquals("b", ((TsValue.Str) got.get(2).orElseThrow()).value());
    }

    @Test
    void lagNGreaterThanOne() {
        TsArray got = TsWindow.lag(twoPartitionFrame(), "val", 2, BY_KEY);
        // partition 1.0 vals 0,2,4 ; lag2 -> null,null,0
        assertTrue(got.get(0).isEmpty());
        assertTrue(got.get(2).isEmpty());
        assertEquals(0.0, ((TsValue.F64) got.get(4).orElseThrow()).value());
    }

    @Test
    void rowNumberIsOneToKPerPartition() {
        TsArray got = TsWindow.rowNumber(twoSymbolFrame(), BY_SYM, null);
        // arrival order within each partition: 1,2,3 each, interleaved on the axis.
        assertEquals(TsDataType.I64, got.dataType());
        assertEquals(ll(1L, 1L, 2L, 2L, 3L, 3L), i64s(got));
    }

    @Test
    void rankHandlesTiesWithGap() {
        // single partition, order_by has ties: values 10,10,20,30 over rows 0..3.
        TsDataFrame f = frame(
                "key", i64Col(ts(0, 1, 2, 3), l(1, 1, 1, 1)),
                "ord", f64Col(ts(0, 1, 2, 3), d(10, 10, 20, 30)));
        TsArray got = TsWindow.rank(f, BY_KEY, "ord");
        // ranks: 1,1,3,4 (tie at rank 1 skips rank 2).
        assertEquals(ll(1L, 1L, 3L, 4L), i64s(got));
    }

    @Test
    void denseRankHandlesTiesWithoutGap() {
        TsDataFrame f = frame(
                "key", i64Col(ts(0, 1, 2, 3), l(1, 1, 1, 1)),
                "ord", f64Col(ts(0, 1, 2, 3), d(10, 10, 20, 30)));
        TsArray got = TsWindow.denseRank(f, BY_KEY, "ord");
        // dense ranks: 1,1,2,3 (no gap).
        assertEquals(ll(1L, 1L, 2L, 3L), i64s(got));
    }

    @Test
    void cumsumPerPartitionMatchesReference() {
        TsArray got = TsWindow.cumsum(twoPartitionFrame(), "val", BY_KEY, null);
        // partition 1.0 vals 0,2,4 -> running 0,2,6 (rows 0,2,4)
        // partition 2.0 vals 1,3,5 -> running 1,4,9 (rows 1,3,5)
        assertEquals(dl(0.0, 1.0, 2.0, 4.0, 6.0, 9.0), f64s(got));
    }

    @Test
    void cumsumOverI64ColumnPromotesToF64() {
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "qty", i64Col(ts(0, 1, 2), l(2, 3, 5)));
        TsArray got = TsWindow.cumsum(f, "qty", BY_SYM, null);
        assertEquals(TsDataType.F64, got.dataType());
        assertEquals(dl(2.0, 5.0, 10.0), f64s(got));
    }

    @Test
    void cumprodPerPartitionMatchesReference() {
        TsDataFrame f = frame(
                "key", f64Col(ts(0, 1, 2), d(1, 1, 1)),
                "val", f64Col(ts(0, 1, 2), d(2, 3, 4)));
        TsArray got = TsWindow.cumprod(f, "val", BY_KEY, null);
        assertEquals(dl(2.0, 6.0, 24.0), f64s(got));
    }

    @Test
    void cumminCummaxPerPartition() {
        TsDataFrame f = frame(
                "key", f64Col(ts(0, 1, 2, 3), d(1, 1, 1, 1)),
                "val", f64Col(ts(0, 1, 2, 3), d(5, 3, 8, 1)));
        TsArray mins = TsWindow.cummin(f, "val", BY_KEY, null);
        TsArray maxs = TsWindow.cummax(f, "val", BY_KEY, null);
        assertEquals(dl(5.0, 3.0, 3.0, 1.0), f64s(mins));
        assertEquals(dl(5.0, 5.0, 8.0, 8.0), f64s(maxs));
    }

    @Test
    void overBroadcastsPartitionAggregateStringKeyed() {
        TsArray got = TsWindow.over(twoSymbolFrame(), TsExpr.col("px").sum(), BY_SYM);
        // AAPL px = 0+2+4 = 6 ; MSFT px = 1+3+5 = 9, broadcast across each row.
        assertEquals(dl(6.0, 9.0, 6.0, 9.0, 6.0, 9.0), f64s(got));
    }

    @Test
    void overCountYieldsI64() {
        TsArray got = TsWindow.over(twoSymbolFrame(), TsExpr.col("px").count(), BY_SYM);
        // each symbol has 3 rows.
        assertEquals(TsDataType.I64, got.dataType());
        assertEquals(ll(3L, 3L, 3L, 3L, 3L, 3L), i64s(got));
    }

    @Test
    void overRejectsNonAggregation() {
        TsDataFrame f = twoSymbolFrame();
        TsWindowException e = assertThrows(TsWindowException.class,
                () -> TsWindow.over(f, TsExpr.col("px"), BY_SYM));
        assertEquals(TsWindowException.Kind.NOT_AN_AGGREGATION, e.kind());
    }

    @Test
    void singlePartitionBehavesAsGlobal() {
        TsDataFrame f = frame(
                "key", f64Col(ts(0, 1, 2), d(7, 7, 7)),
                "val", f64Col(ts(0, 1, 2), d(1, 2, 3)));
        TsArray cs = TsWindow.cumsum(f, "val", BY_KEY, null);
        assertEquals(dl(1.0, 3.0, 6.0), f64s(cs));
        TsArray rn = TsWindow.rowNumber(f, BY_KEY, null);
        assertEquals(ll(1L, 2L, 3L), i64s(rn));
    }

    @Test
    void emptyFrameYieldsEmptyArrays() {
        TsDataFrame f = frame(
                "key", new TsColumn.F64(new TsSeriesD()),
                "val", new TsColumn.F64(new TsSeriesD()));
        assertEquals(0, TsWindow.lag(f, "val", 1, BY_KEY).len());
        assertEquals(0, TsWindow.cumsum(f, "val", BY_KEY, null).len());
        assertEquals(0, TsWindow.rowNumber(f, BY_KEY, null).len());
        assertEquals(0, TsWindow.over(f, TsExpr.col("val").sum(), BY_KEY).len());
    }

    @Test
    void orderByChangesTheResult() {
        // Default order is arrival (ts) order; explicit order_by ascending reverses
        // the running scan relative to the ts axis.
        TsDataFrame f = frame(
                "key", f64Col(ts(0, 1, 2), d(1, 1, 1)),
                "val", f64Col(ts(0, 1, 2), d(1, 2, 4)),
                "ord", f64Col(ts(0, 1, 2), d(30, 20, 10)));
        TsArray dflt = TsWindow.cumsum(f, "val", BY_KEY, null);
        // default ts order: vals 1,2,4 -> 1,3,7
        assertEquals(dl(1.0, 3.0, 7.0), f64s(dflt));

        TsArray ordered = TsWindow.cumsum(f, "val", BY_KEY, "ord");
        // ord ascending: ts2(10,val4), ts1(20,val2), ts0(30,val1).
        // running 4,6,7 scattered back to rows 2,1,0.
        assertEquals(4.0, ((TsValue.F64) ordered.get(2).orElseThrow()).value());
        assertEquals(6.0, ((TsValue.F64) ordered.get(1).orElseThrow()).value());
        assertEquals(7.0, ((TsValue.F64) ordered.get(0).orElseThrow()).value());
    }

    @Test
    void deterministicAcrossRuns() {
        TsDataFrame f = twoSymbolFrame();
        TsArray a = TsWindow.cumsum(f, "px", BY_SYM, null);
        TsArray b = TsWindow.cumsum(f, "px", BY_SYM, null);
        assertEquals(a, b);
    }

    @Test
    void unknownColumnErrors() {
        TsDataFrame f = twoSymbolFrame();
        assertThrows(TsWindowException.class, () -> TsWindow.lag(f, "nope", 1, BY_SYM));
        assertThrows(TsWindowException.class,
                () -> TsWindow.cumsum(f, "px", new String[] {"missingkey"}, null));
        assertThrows(TsWindowException.class, () -> TsWindow.rank(f, BY_SYM, "nope"));
    }

    @Test
    void cumsumRejectsNonNumericColumn() {
        TsDataFrame f = twoSymbolFrame();
        // sym is a Str column; a running sum over it is a type error.
        TsWindowException e = assertThrows(TsWindowException.class,
                () -> TsWindow.cumsum(f, "sym", BY_SYM, null));
        assertEquals(TsWindowException.Kind.NOT_NUMERIC, e.kind());
    }

    @Test
    void cumsumCarriesStateAcrossNullInput() {
        // val has a hole at ts=1 on the aligned axis. The running sum carries the
        // previous total forward; the hole's output stays a valid running total.
        TsDataFrame f = frame(
                "key", f64Col(ts(0, 1, 2), d(1, 1, 1)),
                "val", f64Col(ts(0, 2), d(10, 5))); // no val at ts=1
        TsArray got = TsWindow.cumsum(f, "val", BY_KEY, null);
        // row0: 10 ; row1: input null, acc stays 10 -> valid 10 ; row2: 10+5=15
        assertEquals(dl(10.0, 10.0, 15.0), f64s(got));
    }

    @Test
    void multiTypedKeyPartitioning() {
        // two key columns of DIFFERENT types: a Str venue + an I64 side. Partition
        // is the typed tuple (venue, side).
        TsDataFrame f = frame(
                "venue", strCol(ts(0, 1, 2, 3), str("X", "X", "X", "X")),
                "side", i64Col(ts(0, 1, 2, 3), l(1, 1, 2, 2)),
                "val", f64Col(ts(0, 1, 2, 3), d(1, 2, 3, 4)));
        TsArray got = TsWindow.cumsum(f, "val", new String[] {"venue", "side"}, null);
        // (X,1): rows 0,1 vals 1,2 -> 1,3 ; (X,2): rows 2,3 vals 3,4 -> 3,7
        assertEquals(dl(1.0, 3.0, 3.0, 7.0), f64s(got));
    }

    @Test
    void boolPartitionKeyGroupsOnFlag() {
        // a Bool partition key exercises the BoolKey cell; ordering by the bool
        // flag exercises the orderKey bool branch.
        TsDataFrame f = frame(
                "flag", boolCol(ts(0, 1, 2, 3), b(true, false, true, false)),
                "val", f64Col(ts(0, 1, 2, 3), d(1, 2, 3, 4)));
        TsArray got = TsWindow.cumsum(f, "val", new String[] {"flag"}, "flag");
        // partition true: rows 0,2 vals 1,3 -> 1,4 ; partition false: rows 1,3
        // vals 2,4 -> 2,6.
        assertEquals(1.0, ((TsValue.F64) got.get(0).orElseThrow()).value());
        assertEquals(2.0, ((TsValue.F64) got.get(1).orElseThrow()).value());
        assertEquals(4.0, ((TsValue.F64) got.get(2).orElseThrow()).value());
        assertEquals(6.0, ((TsValue.F64) got.get(3).orElseThrow()).value());
    }

    @Test
    void leadOnStringColumnPreservesType() {
        // lead over a Str column exercises the gather STR branch + the lead path.
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "note", strCol(ts(0, 1, 2), str("x", "y", "z")));
        TsArray got = TsWindow.lead(f, "note", 1, BY_SYM);
        assertEquals(TsDataType.STR, got.dataType());
        assertEquals("y", ((TsValue.Str) got.get(0).orElseThrow()).value());
        assertEquals("z", ((TsValue.Str) got.get(1).orElseThrow()).value());
        assertTrue(got.get(2).isEmpty());
    }

    @Test
    void lagOnBoolColumnPreservesType() {
        // lag over a Bool column exercises the gather BOOL branch.
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "flag", boolCol(ts(0, 1, 2), b(true, false, true)));
        TsArray got = TsWindow.lag(f, "flag", 1, BY_SYM);
        assertEquals(TsDataType.BOOL, got.dataType());
        assertTrue(got.get(0).isEmpty());
        assertEquals(true, ((TsValue.Bool) got.get(1).orElseThrow()).value());
        assertEquals(false, ((TsValue.Bool) got.get(2).orElseThrow()).value());
    }

    @Test
    void overMinOnStringColumnBroadcastsStr() {
        // an over(min) over a Str value column broadcasts a Str scalar, exercising
        // the broadcast STR branch.
        TsDataFrame f = frame(
                "grp", strCol(ts(0, 1, 2, 3), str("g", "g", "h", "h")),
                "tag", strCol(ts(0, 1, 2, 3), str("delta", "alpha", "charlie", "bravo")));
        TsArray got = TsWindow.over(f, TsExpr.col("tag").min(), new String[] {"grp"});
        assertEquals(TsDataType.STR, got.dataType());
        // g: min(delta,alpha)=alpha ; h: min(charlie,bravo)=bravo.
        assertEquals("alpha", ((TsValue.Str) got.get(0).orElseThrow()).value());
        assertEquals("alpha", ((TsValue.Str) got.get(1).orElseThrow()).value());
        assertEquals("bravo", ((TsValue.Str) got.get(2).orElseThrow()).value());
        assertEquals("bravo", ((TsValue.Str) got.get(3).orElseThrow()).value());
    }

    @Test
    void overMeanOfAllNullPartitionIsNullCell() {
        // group b has no v -> mean NaN -> a null cell, exercising the NaN-skip in
        // broadcast's kind probe and the invalid-cell scatter.
        TsDataFrame f = frame(
                "k", strCol(ts(0, 1), str("a", "b")),
                "v", f64Col(ts(0), d(5))); // v only at ts0 (group a); group b has no v.
        TsArray got = TsWindow.over(f, TsExpr.col("v").mean(), new String[] {"k"});
        assertEquals(5.0, ((TsValue.F64) got.get(0).orElseThrow()).value());
        assertTrue(got.get(1).isEmpty());
    }

    @Test
    void nullPartitionKeyFormsItsOwnBucket() {
        // key has a hole at ts=1; that row's key cell is null and lands in a
        // dedicated null bucket, exercising the NullKey cell.
        TsDataFrame f = frame(
                "key", strCol(ts(0, 2, 3), str("A", "A", "A")), // no key at ts=1
                "val", f64Col(ts(0, 1, 2, 3), d(10, 99, 20, 5)));
        TsArray got = TsWindow.cumsum(f, "val", new String[] {"key"}, null);
        // bucket A: rows 0,2,3 vals 10,20,5 -> 10,30,35 ; null bucket: row1 -> 99.
        assertEquals(10.0, ((TsValue.F64) got.get(0).orElseThrow()).value());
        assertEquals(99.0, ((TsValue.F64) got.get(1).orElseThrow()).value());
        assertEquals(30.0, ((TsValue.F64) got.get(2).orElseThrow()).value());
        assertEquals(35.0, ((TsValue.F64) got.get(3).orElseThrow()).value());
    }

    @Test
    void lagOnI64ColumnPreservesType() {
        // lag over an I64 column exercises the gather I64 branch.
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "qty", i64Col(ts(0, 1, 2), l(7, 8, 9)));
        TsArray got = TsWindow.lag(f, "qty", 1, BY_SYM);
        assertEquals(TsDataType.I64, got.dataType());
        assertTrue(got.get(0).isEmpty());
        assertEquals(7L, ((TsValue.I64) got.get(1).orElseThrow()).value());
        assertEquals(8L, ((TsValue.I64) got.get(2).orElseThrow()).value());
    }

    @Test
    void overSumWithI64ColumnInFrameBuildsTypedSubFrame() {
        // the frame carries an extra I64 column the agg does not touch; over still
        // materialises every column into the per-partition sub-frame, exercising
        // the I64 sub-frame branch.
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "qty", i64Col(ts(0, 1, 2), l(1, 2, 3)),
                "px", f64Col(ts(0, 1, 2), d(10, 20, 30)));
        TsArray got = TsWindow.over(f, TsExpr.col("px").sum(), BY_SYM);
        assertEquals(dl(60.0, 60.0, 60.0), f64s(got));
        // and a sum over the I64 column reduces it as numeric.
        TsArray qtySum = TsWindow.over(f, TsExpr.col("qty").sum(), BY_SYM);
        assertEquals(6.0, ((TsValue.F64) qtySum.get(0).orElseThrow()).value());
    }

    @Test
    void overTypeMismatchInAggSurfacesAsNotNumeric() {
        // min over a Bool column is undefined in the evaluator (TYPE_MISMATCH);
        // over remaps that to a NOT_NUMERIC window error (fromExpr path). It also
        // builds a Bool sub-frame column, exercising that sub-frame branch.
        TsDataFrame f = frame(
                "grp", strCol(ts(0, 1, 2, 3), str("g", "g", "h", "h")),
                "ok", boolCol(ts(0, 1, 2, 3), b(true, false, true, true)));
        TsWindowException e = assertThrows(TsWindowException.class,
                () -> TsWindow.over(f, TsExpr.col("ok").min(), new String[] {"grp"}));
        assertEquals(TsWindowException.Kind.NOT_NUMERIC, e.kind());
    }

    @Test
    void orderByI64ColumnRanks() {
        // an I64 order-by column exercises the orderKey I64 branch.
        TsDataFrame f = frame(
                "key", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "val", f64Col(ts(0, 1, 2), d(1, 2, 4)),
                "seq", i64Col(ts(0, 1, 2), l(30, 20, 10)));
        TsArray got = TsWindow.cumsum(f, "val", BY_KEY, "seq");
        // seq ascending: ts2(seq10,val4),ts1(seq20,val2),ts0(seq30,val1).
        // running 4,6,7 scattered to rows 2,1,0.
        assertEquals(4.0, ((TsValue.F64) got.get(2).orElseThrow()).value());
        assertEquals(6.0, ((TsValue.F64) got.get(1).orElseThrow()).value());
        assertEquals(7.0, ((TsValue.F64) got.get(0).orElseThrow()).value());
    }

    @Test
    void orderByWithNullValueSortsFirst() {
        // an order-by column with a hole: the missing order-by cell sorts first
        // (NULLS FIRST), exercising the orderKey null fallthrough.
        TsDataFrame f = frame(
                "key", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "val", f64Col(ts(0, 1, 2), d(1, 2, 4)),
                "ord", f64Col(ts(0, 2), d(50, 10))); // no ord at ts=1
        TsArray got = TsWindow.cumsum(f, "val", BY_KEY, "ord");
        // order: ts1(ord null,val2) first, then ts2(ord10,val4), then ts0(ord50,val1).
        // running 2,6,7 scattered to rows 1,2,0.
        assertEquals(2.0, ((TsValue.F64) got.get(1).orElseThrow()).value());
        assertEquals(6.0, ((TsValue.F64) got.get(2).orElseThrow()).value());
        assertEquals(7.0, ((TsValue.F64) got.get(0).orElseThrow()).value());
    }

    @Test
    void overUnknownColumnSurfacesAsWindowError() {
        // an unknown column referenced inside the agg surfaces through evalScalar
        // and is remapped to a window UnknownColumn error (fromExpr path).
        TsDataFrame f = twoSymbolFrame();
        TsWindowException e = assertThrows(TsWindowException.class,
                () -> TsWindow.over(f, TsExpr.col("ghost").sum(), BY_SYM));
        assertEquals(TsWindowException.Kind.UNKNOWN_COLUMN, e.kind());
    }

    @Test
    void overMinStrWithEmptyFirstPartitionBroadcastsNullThenStr() {
        // group h is first-seen but holds no tag value, so its min is a null
        // scalar; group g reduces to a Str scalar. The null scalar precedes the
        // Str scalar in partition order, so broadcast's kind probe skips the null
        // (Null-continue) before settling on STR, and h's rows scatter as invalid
        // Str cells.
        TsDataFrame f = frame(
                "grp", strCol(ts(0, 1, 2, 3), str("h", "g", "h", "g")),
                "tag", strCol(ts(1, 3), str("delta", "alpha"))); // tag only on g rows
        TsArray got = TsWindow.over(f, TsExpr.col("tag").min(), new String[] {"grp"});
        assertEquals(TsDataType.STR, got.dataType());
        // rows 0,2 are group h (no tag) -> invalid ; rows 1,3 are group g -> alpha.
        assertTrue(got.get(0).isEmpty());
        assertTrue(got.get(2).isEmpty());
        assertEquals("alpha", ((TsValue.Str) got.get(1).orElseThrow()).value());
        assertEquals("alpha", ((TsValue.Str) got.get(3).orElseThrow()).value());
    }

    @Test
    void overMeanWithEmptyFirstPartitionSkipsNanInProbe() {
        // group b is first-seen with no v -> mean NaN; group a reduces to a finite
        // F64. The NaN scalar precedes the finite one, exercising broadcast's
        // NaN-continue in the kind probe. b's rows scatter as invalid F64 cells.
        TsDataFrame f = frame(
                "k", strCol(ts(0, 1, 2, 3), str("b", "a", "b", "a")),
                "v", f64Col(ts(1, 3), d(4, 6))); // v only on a rows
        TsArray got = TsWindow.over(f, TsExpr.col("v").mean(), new String[] {"k"});
        assertEquals(TsDataType.F64, got.dataType());
        assertTrue(got.get(0).isEmpty());
        assertTrue(got.get(2).isEmpty());
        assertEquals(5.0, ((TsValue.F64) got.get(1).orElseThrow()).value());
        assertEquals(5.0, ((TsValue.F64) got.get(3).orElseThrow()).value());
    }

    @Test
    void lagOverAllNullColumnDefaultsToF64Array() {
        // an empty column has no present cell on the aligned axis, so columnKind
        // cannot infer a type and falls through to F64. A lag over it yields an
        // all-invalid F64 array sized to the axis (exercising the columnKind
        // all-null fallthrough).
        TsDataFrame f = frame(
                "sym", strCol(ts(0, 1, 2), str("A", "A", "A")),
                "px", f64Col(ts(0, 1, 2), d(1, 2, 3)),
                "hole", new TsColumn.F64(new TsSeriesD())); // no points: all-null on the axis
        TsArray got = TsWindow.lag(f, "hole", 1, BY_SYM);
        assertEquals(TsDataType.F64, got.dataType());
        assertEquals(dl(null, null, null), f64s(got));
    }
}
