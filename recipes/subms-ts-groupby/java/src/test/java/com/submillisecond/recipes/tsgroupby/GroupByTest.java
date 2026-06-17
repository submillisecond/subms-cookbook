package com.submillisecond.recipes.tsgroupby;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsExpr;

class GroupByTest {

    private static TsColumn strCol(String... vals) {
        TsSeries<String> s = new TsSeries<>();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.Str(s);
    }

    private static TsColumn f64Col(double... vals) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.F64(s);
    }

    private static TsColumn i64Col(long... vals) {
        TsSeriesL s = new TsSeriesL();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.I64(s);
    }

    private static TsDataFrame frame(Object... cols) {
        TsDataFrame f = new TsDataFrame();
        for (int i = 0; i < cols.length; i += 2) {
            f.pushColumn((String) cols[i], (TsColumn) cols[i + 1]);
        }
        return f;
    }

    private static double f64Of(Optional<TsValue> v) {
        return ((TsValue.F64) v.orElseThrow()).value();
    }

    private static long i64Of(Optional<TsValue> v) {
        return ((TsValue.I64) v.orElseThrow()).value();
    }

    private static String strOf(Optional<TsValue> v) {
        return ((TsValue.Str) v.orElseThrow()).value();
    }

    // HEADLINE: group by a STRING key with sum/mean/min/max/count vs a
    // hand-rolled reference. Proves string keying works end to end.
    @Test
    void stringKeyAllAggregationsMatchReference() {
        TsDataFrame f = frame(
                "symbol", strCol("AAPL", "MSFT", "AAPL", "MSFT", "AAPL", "GOOG"),
                "px", f64Col(10, 20, 30, 40, 50, 7));
        TsGroupResult r = GroupBy.groupBy(f, "symbol").agg(
                new TsGroupBy.Agg("sum", TsExpr.col("px").sum()),
                new TsGroupBy.Agg("mean", TsExpr.col("px").mean()),
                new TsGroupBy.Agg("min", TsExpr.col("px").min()),
                new TsGroupBy.Agg("max", TsExpr.col("px").max()),
                new TsGroupBy.Agg("count", TsExpr.col("px").count()));

        // sorted by key: AAPL, GOOG, MSFT.
        assertEquals(3, r.nrows());
        assertEquals("AAPL", strOf(r.value("symbol", 0)));
        assertEquals("GOOG", strOf(r.value("symbol", 1)));
        assertEquals("MSFT", strOf(r.value("symbol", 2)));

        // AAPL: 10,30,50 ; GOOG: 7 ; MSFT: 20,40.
        assertEquals(90.0, f64Of(r.value("sum", 0)));
        assertEquals(30.0, f64Of(r.value("mean", 0)));
        assertEquals(10.0, f64Of(r.value("min", 0)));
        assertEquals(50.0, f64Of(r.value("max", 0)));
        assertEquals(3L, i64Of(r.value("count", 0)));

        assertEquals(7.0, f64Of(r.value("sum", 1)));
        assertEquals(1L, i64Of(r.value("count", 1)));

        assertEquals(60.0, f64Of(r.value("sum", 2)));
        assertEquals(30.0, f64Of(r.value("mean", 2)));
        assertEquals(2L, i64Of(r.value("count", 2)));
    }

    @Test
    void i64KeyGroupsOnInteger() {
        TsDataFrame f = frame("day", i64Col(1, 2, 1, 2, 1), "v", f64Col(10, 20, 30, 40, 50));
        TsGroupResult r = GroupBy.groupBy(f, "day").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertEquals(2, r.nrows());
        assertEquals(1L, i64Of(r.value("day", 0)));
        assertEquals(90.0, f64Of(r.value("s", 0)));
        assertEquals(2L, i64Of(r.value("day", 1)));
        assertEquals(60.0, f64Of(r.value("s", 1)));
    }

    @Test
    void multiKeyStringPlusInt() {
        TsDataFrame f = frame(
                "sym", strCol("A", "A", "B", "A", "B"),
                "side", i64Col(0, 1, 0, 0, 0),
                "v", f64Col(1, 2, 3, 4, 5));
        TsGroupResult r = GroupBy.groupBy(f, "sym", "side").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        // groups (A,0):1+4=5, (A,1):2, (B,0):3+5=8.
        assertEquals(3, r.nrows());
        assertEquals("A", strOf(r.value("sym", 0)));
        assertEquals(0L, i64Of(r.value("side", 0)));
        assertEquals(5.0, f64Of(r.value("s", 0)));
        assertEquals("A", strOf(r.value("sym", 1)));
        assertEquals(1L, i64Of(r.value("side", 1)));
        assertEquals(2.0, f64Of(r.value("s", 1)));
        assertEquals("B", strOf(r.value("sym", 2)));
        assertEquals(8.0, f64Of(r.value("s", 2)));
    }

    @Test
    void aggWithComputedExprPerGroup() {
        TsDataFrame f = frame(
                "sym", strCol("X", "X", "Y"),
                "price", f64Col(2, 3, 4),
                "volume", f64Col(5, 7, 6));
        TsGroupResult r = GroupBy.groupBy(f, "sym").agg(
                new TsGroupBy.Agg("notional", TsExpr.col("price").mul(TsExpr.col("volume")).sum()));
        // X: 2*5 + 3*7 = 31 ; Y: 4*6 = 24.
        assertEquals(31.0, f64Of(r.value("notional", 0)));
        assertEquals(24.0, f64Of(r.value("notional", 1)));
    }

    @Test
    void emptyFrameYieldsNoGroups() {
        TsDataFrame f = frame(
                "k", new TsColumn.Str(new TsSeries<>()),
                "v", new TsColumn.F64(new TsSeriesD()));
        TsGroupResult r = GroupBy.groupBy(f, "k").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertEquals(0, r.nrows());
        assertEquals(2, r.ncols());
    }

    @Test
    void singleGroupWhenKeyIsConstant() {
        TsDataFrame f = frame("k", strCol("one", "one", "one"), "v", f64Col(1, 2, 3));
        TsGroupResult r = GroupBy.groupBy(f, "k").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertEquals(1, r.nrows());
        assertEquals("one", strOf(r.value("k", 0)));
        assertEquals(6.0, f64Of(r.value("s", 0)));
    }

    @Test
    void valueCountsSortedByDescendingCount() {
        TsDataFrame f = frame("k", strCol("a", "b", "a", "a", "b", "c"));
        TsGroupResult vc = GroupBy.valueCounts(f, "k");
        assertEquals(3, vc.nrows());
        assertEquals("a", strOf(vc.value("k", 0)));
        assertEquals(3L, i64Of(vc.value("count", 0)));
        assertEquals("b", strOf(vc.value("k", 1)));
        assertEquals(2L, i64Of(vc.value("count", 1)));
        assertEquals("c", strOf(vc.value("k", 2)));
        assertEquals(1L, i64Of(vc.value("count", 2)));
    }

    @Test
    void uniqueDistinctKeyTuplesSorted() {
        TsDataFrame f = frame("a", strCol("x", "x", "y", "x"), "b", i64Col(9, 9, 8, 7));
        TsGroupResult u = GroupBy.unique(f, "a", "b");
        // distinct sorted -> (x,7),(x,9),(y,8).
        assertEquals(3, u.nrows());
        assertEquals("x", strOf(u.value("a", 0)));
        assertEquals(7L, i64Of(u.value("b", 0)));
        assertEquals("x", strOf(u.value("a", 1)));
        assertEquals(9L, i64Of(u.value("b", 1)));
        assertEquals("y", strOf(u.value("a", 2)));
        assertEquals(8L, i64Of(u.value("b", 2)));
    }

    @Test
    void topKReturnsKLargestRows() {
        TsDataFrame f = frame("v", f64Col(3, 9, 1, 7, 5), "id", f64Col(0, 1, 2, 3, 4));
        TsDataFrame top = GroupBy.topK(f, "v", 2);
        // a reordered frame with the 2 largest v rows: v=9 (id 1), v=7 (id 3).
        assertEquals(2, top.aligned().size());
        assertEquals(9.0, ((TsValue.F64) firstCell(top, "v", 0)).value());
        assertEquals(1.0, ((TsValue.F64) firstCell(top, "id", 0)).value());
        assertEquals(7.0, ((TsValue.F64) firstCell(top, "v", 1)).value());
        assertEquals(3.0, ((TsValue.F64) firstCell(top, "id", 1)).value());
    }

    @Test
    void topKClampsToAvailableRows() {
        TsDataFrame f = frame("v", f64Col(1, 2));
        TsDataFrame top = GroupBy.topK(f, "v", 10);
        assertEquals(2, top.aligned().size());
        assertEquals(2.0, ((TsValue.F64) firstCell(top, "v", 0)).value());
        assertEquals(1.0, ((TsValue.F64) firstCell(top, "v", 1)).value());
    }

    @Test
    void sortByAscendingAndDescending() {
        TsDataFrame f = frame("v", f64Col(3, 1, 2), "id", f64Col(10, 11, 12));
        TsDataFrame asc = GroupBy.sortBy(f, true, "v");
        assertEquals(1.0, ((TsValue.F64) firstCell(asc, "v", 0)).value());
        assertEquals(11.0, ((TsValue.F64) firstCell(asc, "id", 0)).value());
        assertEquals(3.0, ((TsValue.F64) firstCell(asc, "v", 2)).value());

        TsDataFrame desc = GroupBy.sortBy(f, false, "v");
        assertEquals(3.0, ((TsValue.F64) firstCell(desc, "v", 0)).value());
        assertEquals(1.0, ((TsValue.F64) firstCell(desc, "v", 2)).value());
    }

    @Test
    void sortByMultiKeyLexicographicOnStringThenInt() {
        TsDataFrame f = frame("sym", strCol("B", "A", "A"), "seq", i64Col(1, 5, 2));
        TsDataFrame s = GroupBy.sortBy(f, true, "sym", "seq");
        // sorted by sym then seq: (A,2),(A,5),(B,1).
        assertEquals("A", ((TsValue.Str) firstCell(s, "sym", 0)).value());
        assertEquals(2L, ((TsValue.I64) firstCell(s, "seq", 0)).value());
        assertEquals("A", ((TsValue.Str) firstCell(s, "sym", 1)).value());
        assertEquals(5L, ((TsValue.I64) firstCell(s, "seq", 1)).value());
        assertEquals("B", ((TsValue.Str) firstCell(s, "sym", 2)).value());
        assertEquals(1L, ((TsValue.I64) firstCell(s, "seq", 2)).value());
    }

    @Test
    void nullKeyRowsAreDropped() {
        // symbol has a hole at ts=1 (size carries it). That row's key is null.
        TsSeries<String> symbol = new TsSeries<>();
        symbol.push(0, "A");
        symbol.push(2, "A");
        symbol.push(3, "B");
        TsSeriesD size = new TsSeriesD();
        size.push(0, 10);
        size.push(1, 99);
        size.push(2, 20);
        size.push(3, 5);
        TsDataFrame f = frame(
                "symbol", new TsColumn.Str(symbol),
                "size", new TsColumn.F64(size));
        TsGroupResult r = GroupBy.groupBy(f, "symbol").agg(
                new TsGroupBy.Agg("s", TsExpr.col("size").sum()));
        // A -> 10+20 = 30 ; B -> 5. ts1 (null key) dropped.
        assertEquals(2, r.nrows());
        assertEquals(30.0, f64Of(r.value("s", 0)));
        assertEquals(5.0, f64Of(r.value("s", 1)));
    }

    @Test
    void groupOrderIsDeterministicRegardlessOfInputOrder() {
        TsDataFrame f1 = frame("k", strCol("c", "a", "b"), "v", f64Col(1, 1, 1));
        TsDataFrame f2 = frame("k", strCol("a", "b", "c"), "v", f64Col(1, 1, 1));
        TsGroupResult r1 = GroupBy.groupBy(f1, "k").agg(
                new TsGroupBy.Agg("c", TsExpr.col("v").count()));
        TsGroupResult r2 = GroupBy.groupBy(f2, "k").agg(
                new TsGroupBy.Agg("c", TsExpr.col("v").count()));
        assertEquals("a", strOf(r1.value("k", 0)));
        assertEquals("b", strOf(r1.value("k", 1)));
        assertEquals("c", strOf(r1.value("k", 2)));
        for (int i = 0; i < 3; i++) {
            assertEquals(strOf(r1.value("k", i)), strOf(r2.value("k", i)));
            assertEquals(i64Of(r1.value("c", i)), i64Of(r2.value("c", i)));
        }
    }

    @Test
    void meanOverValidOnlyWithAGap() {
        TsSeries<String> sym = new TsSeries<>();
        sym.push(0, "A");
        sym.push(1, "A");
        sym.push(2, "A");
        sym.push(3, "B");
        TsSeriesD v = new TsSeriesD();
        v.push(0, 4);   // A
        v.push(2, 8);   // A (ts1 gap)
        v.push(3, 100); // B
        TsDataFrame f = frame("sym", new TsColumn.Str(sym), "v", new TsColumn.F64(v));
        TsGroupResult r = GroupBy.groupBy(f, "sym").agg(
                new TsGroupBy.Agg("mean", TsExpr.col("v").mean()),
                new TsGroupBy.Agg("count", TsExpr.col("v").count()));
        // A: valid 4 and 8 (ts1 gap excluded) -> mean 6, count 2.
        assertEquals(6.0, f64Of(r.value("mean", 0)));
        assertEquals(2L, i64Of(r.value("count", 0)));
        assertEquals(100.0, f64Of(r.value("mean", 1)));
        assertEquals(1L, i64Of(r.value("count", 1)));
    }

    @Test
    void aggOverGroupWithAllNullTargetIsNullCell() {
        TsSeries<String> k = new TsSeries<>();
        k.push(0, "a");
        k.push(1, "b");
        TsSeriesD v = new TsSeriesD();
        v.push(0, 5); // v only at ts0 (group a); group b has no v.
        TsDataFrame f = frame("k", new TsColumn.Str(k), "v", new TsColumn.F64(v));
        TsGroupResult r = GroupBy.groupBy(f, "k").agg(
                new TsGroupBy.Agg("m", TsExpr.col("v").mean()),
                new TsGroupBy.Agg("c", TsExpr.col("v").count()));
        assertEquals(5.0, f64Of(r.value("m", 0)));
        assertEquals(1L, i64Of(r.value("c", 0)));
        // group b: no v -> mean NaN -> null cell ; count 0.
        assertTrue(r.value("m", 1).isEmpty());
        assertEquals(0L, i64Of(r.value("c", 1)));
    }

    @Test
    void emptyKeysThrows() {
        TsDataFrame f = frame("k", strCol("a"));
        GroupByException e = assertThrows(GroupByException.class, () -> GroupBy.groupBy(f));
        assertEquals(GroupByException.Kind.NO_KEYS, e.kind());
    }

    @Test
    void unknownKeyColumnThrows() {
        TsDataFrame f = frame("k", strCol("a"));
        GroupByException e = assertThrows(GroupByException.class, () -> GroupBy.groupBy(f, "nope"));
        assertEquals(GroupByException.Kind.UNKNOWN_COLUMN, e.kind());
    }

    @Test
    void nonAggExprRejected() {
        TsDataFrame f = frame("k", strCol("a"), "v", f64Col(2));
        GroupByException e = assertThrows(GroupByException.class,
                () -> GroupBy.groupBy(f, "k").agg(new TsGroupBy.Agg("bad", TsExpr.col("v"))));
        assertEquals(GroupByException.Kind.NOT_AN_AGGREGATION, e.kind());
    }

    private static TsColumn boolCol(boolean... vals) {
        TsSeries<Boolean> s = new TsSeries<>();
        for (int i = 0; i < vals.length; i++) {
            s.push(i, vals[i]);
        }
        return new TsColumn.Bool(s);
    }

    @Test
    void boolKeyGroupsOnBoolean() {
        TsDataFrame f = frame("flag", boolCol(true, false, true, false), "v", f64Col(1, 2, 3, 4));
        TsGroupResult r = GroupBy.groupBy(f, "flag").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        // sorted false < true: false -> 2+4=6, true -> 1+3=4.
        assertEquals(2, r.nrows());
        assertEquals(false, ((TsValue.Bool) r.value("flag", 0).orElseThrow()).value());
        assertEquals(6.0, f64Of(r.value("s", 0)));
        assertEquals(true, ((TsValue.Bool) r.value("flag", 1).orElseThrow()).value());
        assertEquals(4.0, f64Of(r.value("s", 1)));
    }

    @Test
    void f64KeyGroupsOnDouble() {
        TsDataFrame f = frame("bucket", f64Col(1.5, 2.5, 1.5), "v", f64Col(10, 20, 30));
        TsGroupResult r = GroupBy.groupBy(f, "bucket").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertEquals(2, r.nrows());
        assertEquals(1.5, f64Of(r.value("bucket", 0)));
        assertEquals(40.0, f64Of(r.value("s", 0)));
        assertEquals(2.5, f64Of(r.value("bucket", 1)));
        assertEquals(20.0, f64Of(r.value("s", 1)));
    }

    @Test
    void aggMinMaxOverStringColumn() {
        // a string aggregation column: min / max pick the lexicographic extreme,
        // exercising the Str branch of the per-group scalar array.
        TsDataFrame f = frame(
                "grp", strCol("g", "g", "h"),
                "tag", strCol("delta", "alpha", "charlie"));
        TsGroupResult r = GroupBy.groupBy(f, "grp").agg(
                new TsGroupBy.Agg("lo", TsExpr.col("tag").min()),
                new TsGroupBy.Agg("hi", TsExpr.col("tag").max()));
        assertEquals("alpha", strOf(r.value("lo", 0)));
        assertEquals("delta", strOf(r.value("hi", 0)));
        assertEquals("charlie", strOf(r.value("lo", 1)));
        assertEquals("charlie", strOf(r.value("hi", 1)));
    }

    @Test
    void valueCountsOnBoolColumn() {
        // value_counts over a bool key exercises the Bool key-array branch.
        TsDataFrame f = frame("flag", boolCol(true, true, false));
        TsGroupResult vc = GroupBy.valueCounts(f, "flag");
        assertEquals(2, vc.nrows());
        // true:2 (descending count first), false:1.
        assertEquals(true, ((TsValue.Bool) vc.value("flag", 0).orElseThrow()).value());
        assertEquals(2L, i64Of(vc.value("count", 0)));
        assertEquals(false, ((TsValue.Bool) vc.value("flag", 1).orElseThrow()).value());
    }

    @Test
    void valueCountsOnI64Column() {
        TsDataFrame f = frame("id", i64Col(7, 7, 9, 7, 9));
        TsGroupResult vc = GroupBy.valueCounts(f, "id");
        assertEquals(7L, i64Of(vc.value("id", 0)));
        assertEquals(3L, i64Of(vc.value("count", 0)));
        assertEquals(9L, i64Of(vc.value("id", 1)));
        assertEquals(2L, i64Of(vc.value("count", 1)));
    }

    @Test
    void sortBySingleI64KeyDescending() {
        TsDataFrame f = frame("seq", i64Col(3, 1, 2));
        TsDataFrame s = GroupBy.sortBy(f, false, "seq");
        assertEquals(3L, ((TsValue.I64) firstCell(s, "seq", 0)).value());
        assertEquals(2L, ((TsValue.I64) firstCell(s, "seq", 1)).value());
        assertEquals(1L, ((TsValue.I64) firstCell(s, "seq", 2)).value());
    }

    @Test
    void topKOnI64Column() {
        TsDataFrame f = frame("score", i64Col(30, 90, 10, 70));
        TsDataFrame top = GroupBy.topK(f, "score", 2);
        assertEquals(2, top.aligned().size());
        assertEquals(90L, ((TsValue.I64) firstCell(top, "score", 0)).value());
        assertEquals(70L, ((TsValue.I64) firstCell(top, "score", 1)).value());
    }

    @Test
    void sortByEmptyColumnsThrows() {
        TsDataFrame f = frame("k", strCol("a"));
        GroupByException e = assertThrows(GroupByException.class, () -> GroupBy.sortBy(f, true));
        assertEquals(GroupByException.Kind.NO_KEYS, e.kind());
    }

    @Test
    void topKUnknownColumnThrows() {
        TsDataFrame f = frame("k", strCol("a"));
        GroupByException e = assertThrows(GroupByException.class, () -> GroupBy.topK(f, "nope", 1));
        assertEquals(GroupByException.Kind.UNKNOWN_COLUMN, e.kind());
    }

    @Test
    void sortByUnknownColumnThrows() {
        TsDataFrame f = frame("k", strCol("a"));
        GroupByException e =
                assertThrows(GroupByException.class, () -> GroupBy.sortBy(f, true, "nope"));
        assertEquals(GroupByException.Kind.UNKNOWN_COLUMN, e.kind());
    }

    @Test
    void unknownAggColumnThrows() {
        TsDataFrame f = frame("k", strCol("a", "b"), "v", f64Col(1, 2));
        GroupByException e = assertThrows(GroupByException.class,
                () -> GroupBy.groupBy(f, "k").agg(new TsGroupBy.Agg("s", TsExpr.col("ghost").sum())));
        assertEquals(GroupByException.Kind.UNKNOWN_COLUMN, e.kind());
    }

    @Test
    void resultColumnAccessorAndKeyTuple() {
        TsDataFrame f = frame("k", strCol("a", "b"), "v", f64Col(1, 2));
        TsGroupBy gb = GroupBy.groupBy(f, "k");
        assertEquals(2, gb.ngroups());
        assertEquals("a", ((TsValue.Str) gb.key(0).get(0)).value());
        assertEquals(1, gb.groupSize(0));
        TsGroupResult r = gb.agg(new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertTrue(r.column("s").isPresent());
        assertTrue(r.column("absent").isEmpty());
        assertTrue(r.value("absent", 0).isEmpty());
        assertTrue(r.value("s", 99).isEmpty());
    }

    @Test
    void columnNamesAreKeysThenAggs() {
        TsDataFrame f = frame("k", strCol("a", "b"), "v", f64Col(1, 2));
        TsGroupResult r = GroupBy.groupBy(f, "k").agg(
                new TsGroupBy.Agg("s", TsExpr.col("v").sum()));
        assertEquals(List.of("k", "s"), r.columnNames());
    }

    // Read the value of a reordered frame's column at a positional row via the
    // aligned view (rows are at synthetic ts 0..n in result order).
    private static TsValue firstCell(TsDataFrame f, String col, int row) {
        int ci = f.columnNames().indexOf(col);
        return f.aligned().get(row).values().get(ci).orElseThrow();
    }
}
