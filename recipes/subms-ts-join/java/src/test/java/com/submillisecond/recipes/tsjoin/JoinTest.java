package com.submillisecond.recipes.tsjoin;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HashSet;
import java.util.Optional;
import java.util.Set;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;
import com.submillisecond.recipes.ts.TsValue;
import com.submillisecond.recipes.tsexpr.TsArray;

class JoinTest {

    // ---------- frame builders ----------

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

    // left frame: sym (Str) + px (F64).
    private static TsDataFrame quotes() {
        return new TsDataFrame()
                .withColumn("sym", strCol("AAPL", "MSFT", "GOOG"))
                .withColumn("px", f64Col(10.0, 20.0, 30.0));
    }

    // right frame: sym (Str) + qty (F64). AAPL + GOOG match; AMZN is right-only.
    private static TsDataFrame trades() {
        return new TsDataFrame()
                .withColumn("sym", strCol("AAPL", "GOOG", "AMZN"))
                .withColumn("qty", f64Col(100.0, 300.0, 400.0));
    }

    private static String strAt(TsJoinResult r, String col, int row) {
        Optional<TsValue> v = r.column(col).orElseThrow().get(row);
        return v.isPresent() && v.get() instanceof TsValue.Str s ? s.value() : null;
    }

    private static Double f64At(TsJoinResult r, String col, int row) {
        Optional<TsValue> v = r.column(col).orElseThrow().get(row);
        return v.isPresent() && v.get() instanceof TsValue.F64 f ? f.value() : null;
    }

    private static Long i64At(TsJoinResult r, String col, int row) {
        Optional<TsValue> v = r.column(col).orElseThrow().get(row);
        return v.isPresent() && v.get() instanceof TsValue.I64 i ? i.value() : null;
    }

    // A set of joined rows as comparable strings, order-independent.
    private static Set<String> rowSet(TsJoinResult r) {
        Set<String> out = new HashSet<>();
        for (int row = 0; row < r.nrows(); row++) {
            StringBuilder sb = new StringBuilder();
            for (String n : r.columnNames()) {
                sb.append(n).append('=').append(r.column(n).orElseThrow().get(row)).append('|');
            }
            out.add(sb.toString());
        }
        return out;
    }

    // ---------- 1. headline: inner join on a STRING key ----------

    @Test
    void innerJoinOnStringKeyMatchesReference() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);

        assertEquals(2, out.nrows());
        Set<String> syms = new HashSet<>();
        for (int r = 0; r < out.nrows(); r++) {
            syms.add(strAt(out, "sym", r));
        }
        assertEquals(Set.of("AAPL", "GOOG"), syms);

        for (int r = 0; r < out.nrows(); r++) {
            switch (strAt(out, "sym", r)) {
                case "AAPL" -> {
                    assertEquals(10.0, f64At(out, "px", r));
                    assertEquals(100.0, f64At(out, "qty", r));
                }
                case "GOOG" -> {
                    assertEquals(30.0, f64At(out, "px", r));
                    assertEquals(300.0, f64At(out, "qty", r));
                }
                default -> throw new AssertionError("unexpected symbol");
            }
        }
    }

    // ---------- 2. left join fills unmatched-right cells NULL ----------

    @Test
    void leftJoinNullsUnmatchedRight() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.LEFT);
        assertEquals(3, out.nrows());
        int msft = -1;
        for (int r = 0; r < out.nrows(); r++) {
            if ("MSFT".equals(strAt(out, "sym", r))) {
                msft = r;
            }
        }
        assertTrue(msft >= 0);
        assertEquals(20.0, f64At(out, "px", msft));
        assertTrue(out.column("qty").orElseThrow().get(msft).isEmpty());
        assertFalse(out.column("qty").orElseThrow().valid()[msft]);
    }

    // ---------- 3. right join ----------

    @Test
    void rightJoinNullsUnmatchedLeft() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.RIGHT);
        assertEquals(3, out.nrows());
        int amzn = -1;
        for (int r = 0; r < out.nrows(); r++) {
            if ("AMZN".equals(strAt(out, "sym", r))) {
                amzn = r;
            }
        }
        assertTrue(amzn >= 0);
        assertTrue(out.column("px").orElseThrow().get(amzn).isEmpty());
        assertEquals(400.0, f64At(out, "qty", amzn));
    }

    // ---------- 4. outer join ----------

    @Test
    void outerJoinKeepsBothUnmatchedSides() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.OUTER);
        assertEquals(4, out.nrows());
        Set<String> syms = new HashSet<>();
        for (int r = 0; r < out.nrows(); r++) {
            syms.add(strAt(out, "sym", r));
        }
        assertEquals(Set.of("AAPL", "MSFT", "GOOG", "AMZN"), syms);
    }

    // ---------- 5. semi join ----------

    @Test
    void semiJoinEmitsMatchingLeftRowsOnly() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.SEMI);
        assertEquals(2, out.nrows());
        assertTrue(out.columnNames().contains("sym"));
        assertTrue(out.columnNames().contains("px"));
        assertFalse(out.columnNames().contains("qty"));
    }

    // ---------- 6. anti join ----------

    @Test
    void antiJoinEmitsUnmatchedLeftRowsOnly() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.ANTI);
        assertEquals(1, out.nrows());
        assertEquals("MSFT", strAt(out, "sym", 0));
        assertFalse(out.columnNames().contains("qty"));
    }

    // ---------- 7. cross join ----------

    @Test
    void crossJoinIsCartesianProduct() {
        TsJoinResult out = TsJoin.crossJoin(quotes(), trades());
        assertEquals(9, out.nrows());
        assertTrue(out.columnNames().contains("sym_left"));
        assertTrue(out.columnNames().contains("sym_right"));
        for (int r = 0; r < out.nrows(); r++) {
            assertTrue(out.column("sym_left").orElseThrow().get(r).isPresent());
            assertTrue(out.column("sym_right").orElseThrow().get(r).isPresent());
        }
    }

    // ---------- 8. multi-key join: STRING + INT ----------

    @Test
    void multiKeyJoinOnStringAndInt() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("sym", strCol("AAPL", "AAPL", "MSFT"))
                .withColumn("day", i64Col(1, 2, 1))
                .withColumn("px", f64Col(10.0, 11.0, 20.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("sym", strCol("AAPL", "MSFT"))
                .withColumn("day", i64Col(1, 1))
                .withColumn("qty", f64Col(100.0, 200.0));

        TsJoinResult out = TsJoin.hashJoin(
                left, right, new String[] {"sym", "day"}, new String[] {"sym", "day"}, TsJoinKind.INNER);
        assertEquals(2, out.nrows());
        for (int r = 0; r < out.nrows(); r++) {
            assertEquals(1L, i64At(out, "day", r));
            switch (strAt(out, "sym", r)) {
                case "AAPL" -> assertEquals(100.0, f64At(out, "qty", r));
                case "MSFT" -> assertEquals(200.0, f64At(out, "qty", r));
                default -> throw new AssertionError("unexpected symbol");
            }
        }
    }

    // ---------- 9. sort-merge == hash result SET across all kinds ----------

    @Test
    void sortMergeMatchesHashForEveryKind() {
        for (TsJoinKind k : TsJoinKind.values()) {
            TsJoinResult h =
                    TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, k);
            TsJoinResult m =
                    TsJoin.sortMergeJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, k);
            assertEquals(h.nrows(), m.nrows(), k + " nrows differ");
            assertEquals(rowSet(h), rowSet(m), k + " row sets differ");
        }
    }

    // ---------- 10. empty side ----------

    @Test
    void emptyRightInnerIsEmptyLeftKeepsAll() {
        TsDataFrame empty = new TsDataFrame()
                .withColumn("sym", strCol())
                .withColumn("qty", f64Col());
        TsJoinResult inner =
                TsJoin.hashJoin(quotes(), empty, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertTrue(inner.isEmpty());
        TsJoinResult left =
                TsJoin.hashJoin(quotes(), empty, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.LEFT);
        assertEquals(3, left.nrows());
        for (int r = 0; r < left.nrows(); r++) {
            assertTrue(left.column("qty").orElseThrow().get(r).isEmpty());
        }
    }

    // ---------- 11. no matches at all ----------

    @Test
    void disjointKeysProduceNoInnerRows() {
        TsDataFrame other = new TsDataFrame()
                .withColumn("sym", strCol("TSLA", "NVDA"))
                .withColumn("qty", f64Col(1.0, 2.0));
        TsJoinResult inner =
                TsJoin.hashJoin(quotes(), other, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertTrue(inner.isEmpty());
        TsJoinResult outer =
                TsJoin.hashJoin(quotes(), other, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.OUTER);
        assertEquals(5, outer.nrows());
    }

    // ---------- 12. duplicate keys: one-to-many ----------

    @Test
    void duplicateKeysFanOutOneToMany() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("sym", strCol("AAPL"))
                .withColumn("px", f64Col(10.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("sym", strCol("AAPL", "AAPL", "AAPL"))
                .withColumn("qty", f64Col(1.0, 2.0, 3.0));
        TsJoinResult out =
                TsJoin.hashJoin(left, right, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertEquals(3, out.nrows());
        // right-input order preserved.
        assertEquals(1.0, f64At(out, "qty", 0));
        assertEquals(2.0, f64At(out, "qty", 1));
        assertEquals(3.0, f64At(out, "qty", 2));
    }

    // ---------- 13. collision suffixing on payload columns ----------

    @Test
    void payloadNameCollisionIsSuffixed() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("sym", strCol("AAPL"))
                .withColumn("vol", f64Col(1.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("sym", strCol("AAPL"))
                .withColumn("vol", f64Col(2.0));
        TsJoinResult out =
                TsJoin.hashJoin(left, right, new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertTrue(out.columnNames().contains("vol_left"));
        assertTrue(out.columnNames().contains("vol_right"));
        assertEquals(1.0, f64At(out, "vol_left", 0));
        assertEquals(2.0, f64At(out, "vol_right", 0));
    }

    // ---------- 14. deterministic output order (left-driving) ----------

    @Test
    void hashJoinOutputIsLeftDrivingDeterministic() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertEquals("AAPL", strAt(out, "sym", 0));
        assertEquals("GOOG", strAt(out, "sym", 1));
        TsJoinResult again =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertEquals(strAt(again, "sym", 0), strAt(out, "sym", 0));
        assertEquals(strAt(again, "sym", 1), strAt(out, "sym", 1));
    }

    // ---------- 15. error surface ----------

    @Test
    void unknownKeyAndArityAndNoKeysError() {
        TsDataFrame q = quotes();
        TsDataFrame t = trades();
        TsJoinException unknown = assertThrows(
                TsJoinException.class,
                () -> TsJoin.hashJoin(q, t, new String[] {"nope"}, new String[] {"sym"}, TsJoinKind.INNER));
        assertEquals(TsJoinException.Kind.UNKNOWN_KEY, unknown.kind());
        TsJoinException arity = assertThrows(
                TsJoinException.class,
                () -> TsJoin.hashJoin(q, t, new String[] {"sym"}, new String[] {"sym", "qty"}, TsJoinKind.INNER));
        assertEquals(TsJoinException.Kind.KEY_ARITY_MISMATCH, arity.kind());
        TsJoinException noKeys = assertThrows(
                TsJoinException.class,
                () -> TsJoin.hashJoin(q, t, new String[0], new String[0], TsJoinKind.INNER));
        assertEquals(TsJoinException.Kind.NO_KEYS, noKeys.kind());
    }

    // ---------- 16. join on an INT key alone ----------

    @Test
    void intKeyJoinWorks() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("day", i64Col(1, 2, 3))
                .withColumn("px", f64Col(10.0, 20.0, 30.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("day", i64Col(2, 3, 4))
                .withColumn("qty", f64Col(200.0, 300.0, 400.0));
        TsJoinResult out =
                TsJoin.hashJoin(left, right, new String[] {"day"}, new String[] {"day"}, TsJoinKind.INNER);
        assertEquals(2, out.nrows());
    }

    // ---------- 17. frameColumns flattening is exposed + typed ----------

    @Test
    void frameColumnsExposesTypedDenseArrays() {
        TsJoin.FrameColumns fc = TsJoin.frameColumns(quotes());
        assertEquals(2, fc.columns().size());
        int symIdx = fc.names().indexOf("sym");
        assertTrue(fc.columns().get(symIdx) instanceof TsArray.Str);
        assertEquals(3, fc.columns().get(symIdx).len());
        int pxIdx = fc.names().indexOf("px");
        assertTrue(fc.columns().get(pxIdx) instanceof TsArray.F64);
    }

    // ---------- 18. sort-merge over int keys equals hash ----------

    @Test
    void sortMergeIntKeyInnerEqualsHash() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("day", i64Col(3, 1, 2))
                .withColumn("px", f64Col(30.0, 10.0, 20.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("day", i64Col(2, 1))
                .withColumn("qty", f64Col(200.0, 100.0));
        TsJoinResult h =
                TsJoin.hashJoin(left, right, new String[] {"day"}, new String[] {"day"}, TsJoinKind.INNER);
        TsJoinResult m =
                TsJoin.sortMergeJoin(left, right, new String[] {"day"}, new String[] {"day"}, TsJoinKind.INNER);
        assertEquals(rowSet(h), rowSet(m));
        assertEquals(2, h.nrows());
    }

    // ---------- 19. bool key join + bool payload null ----------

    @Test
    void boolKeyJoinAndBoolPayloadNull() {
        // key on a boolean flag; right also carries a bool payload column.
        TsDataFrame left = new TsDataFrame()
                .withColumn("flag", boolCol(true, false))
                .withColumn("px", f64Col(10.0, 20.0));
        TsDataFrame right = new TsDataFrame()
                .withColumn("flag", boolCol(true))
                .withColumn("live", boolCol(true));
        TsJoinResult out =
                TsJoin.hashJoin(left, right, new String[] {"flag"}, new String[] {"flag"}, TsJoinKind.LEFT);
        assertEquals(2, out.nrows());
        // the flag=false left row has no right match -> live is NULL.
        int falseRow = -1;
        for (int r = 0; r < out.nrows(); r++) {
            Optional<TsValue> v = out.column("flag").orElseThrow().get(r);
            if (v.isPresent() && v.get() instanceof TsValue.Bool b && !b.value()) {
                falseRow = r;
            }
        }
        assertTrue(falseRow >= 0);
        assertTrue(out.column("live").orElseThrow().get(falseRow).isEmpty());
    }

    // ---------- 20. f64 key join (bit-pattern token) + i64 payload null ----------

    @Test
    void f64KeyJoinAndIntPayloadNull() {
        TsDataFrame left = new TsDataFrame()
                .withColumn("strike", f64Col(100.0, 105.0))
                .withColumn("lot", i64Col(1, 2));
        TsDataFrame right = new TsDataFrame()
                .withColumn("strike", f64Col(100.0))
                .withColumn("oi", i64Col(500));
        TsJoinResult out =
                TsJoin.hashJoin(left, right, new String[] {"strike"}, new String[] {"strike"}, TsJoinKind.LEFT);
        assertEquals(2, out.nrows());
        // strike=105 has no right match -> oi (i64) is NULL.
        int unmatched = -1;
        for (int r = 0; r < out.nrows(); r++) {
            Optional<TsValue> v = out.column("strike").orElseThrow().get(r);
            if (v.isPresent() && v.get() instanceof TsValue.F64 f && f.value() == 105.0) {
                unmatched = r;
            }
        }
        assertTrue(unmatched >= 0);
        assertTrue(out.column("oi").orElseThrow().get(unmatched).isEmpty());
        // the matched row carries oi=500.
        for (int r = 0; r < out.nrows(); r++) {
            if (r != unmatched) {
                assertEquals(500L, i64At(out, "oi", r));
            }
        }
    }

    // ---------- 21. columnAt + accessor surface ----------

    @Test
    void columnAtAndAccessors() {
        TsJoinResult out =
                TsJoin.hashJoin(quotes(), trades(), new String[] {"sym"}, new String[] {"sym"}, TsJoinKind.INNER);
        assertEquals(3, out.ncols()); // sym, px, qty
        assertTrue(out.columnAt(0).isPresent());
        assertTrue(out.columnAt(-1).isEmpty());
        assertTrue(out.columnAt(99).isEmpty());
        assertTrue(out.column("missing").isEmpty());
    }
}
