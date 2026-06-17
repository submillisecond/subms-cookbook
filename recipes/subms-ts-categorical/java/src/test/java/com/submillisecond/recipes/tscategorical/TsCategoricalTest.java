package com.submillisecond.recipes.tscategorical;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;

class TsCategoricalTest {

    // ---------- TsStringInterner ----------

    @Test
    void internSameStringSameId() {
        TsStringInterner it = new TsStringInterner();
        assertEquals(it.intern("AAPL"), it.intern("AAPL"));
        assertEquals(1, it.size());
    }

    @Test
    void internDistinctStringsDistinctIds() {
        TsStringInterner it = new TsStringInterner();
        assertNotEquals(it.intern("AAPL"), it.intern("MSFT"));
        assertEquals(2, it.size());
    }

    @Test
    void internIdsDenseFromZeroInFirstSeenOrder() {
        TsStringInterner it = new TsStringInterner();
        assertEquals(0, it.intern("c"));
        assertEquals(1, it.intern("a"));
        assertEquals(2, it.intern("b"));
        assertEquals(1, it.intern("a")); // repeat
        assertEquals(0, it.intern("c"));
        assertEquals(3, it.intern("d"));
        assertEquals(4, it.size());
    }

    @Test
    void resolveRoundTripsEveryId() {
        TsStringInterner it = new TsStringInterner();
        for (String s : List.of("x", "y", "z")) {
            int id = it.intern(s);
            assertEquals(Optional.of(s), it.resolve(id));
        }
        assertEquals(Optional.empty(), it.resolve(999));
        assertEquals(Optional.empty(), it.resolve(-1));
    }

    @Test
    void containsAndGetReflectMembership() {
        TsStringInterner it = new TsStringInterner();
        assertFalse(it.contains("AAPL"));
        assertEquals(Optional.empty(), it.get("AAPL"));
        int id = it.intern("AAPL");
        assertTrue(it.contains("AAPL"));
        assertEquals(Optional.of(id), it.get("AAPL"));
        assertEquals(Optional.empty(), it.get("MSFT")); // get does not assign
        assertEquals(1, it.size());
    }

    @Test
    void internerEmptyState() {
        TsStringInterner it = new TsStringInterner(8);
        assertTrue(it.isEmpty());
        assertEquals(0, it.size());
        assertTrue(it.strings().isEmpty());
    }

    @Test
    void internerStringsAreInIdOrder() {
        TsStringInterner it = new TsStringInterner();
        it.intern("first");
        it.intern("second");
        it.intern("first");
        assertEquals(List.of("first", "second"), it.strings());
    }

    // ---------- TsDictColumn ----------

    @Test
    void encodeStringSeriesCodesAndCardinality() {
        TsSeries<String> s = new TsSeries<>();
        int i = 0;
        for (String v : List.of("a", "b", "a", "c", "b")) {
            s.push(i++, v);
        }
        TsDictColumn col = TsDictColumn.encode(s);
        assertEquals(5, col.size());
        assertEquals(3, col.cardinality());
        assertEquals(List.of("a", "b", "c"), col.dict());
        assertArrayEquals(new int[] {0, 1, 0, 2, 1}, col.codes());
    }

    @Test
    void decodeAtAndToSeriesRoundTripValues() {
        List<String> original = List.of("x", "y", "x", "z", "y", "x");
        TsDictColumn col = TsDictColumn.fromStrings(original);
        for (int i = 0; i < original.size(); i++) {
            assertEquals(Optional.of(original.get(i)), col.decodeAt(i));
        }
        assertEquals(Optional.empty(), col.decodeAt(original.size()));
        assertEquals(Optional.empty(), col.decodeAt(-1));
        List<String> decoded = new ArrayList<>();
        for (TsPoint<String> p : col.toSeries()) {
            decoded.add(p.value());
        }
        assertEquals(original, decoded);
        assertEquals(original, col.toList());
    }

    @Test
    void highDuplicationColumnYieldsSmallDictionary() {
        String[] symbols = {"AAPL", "MSFT", "GOOG", "AMZN"};
        List<String> values = new ArrayList<>(1000);
        for (int i = 0; i < 1000; i++) {
            values.add(symbols[i % symbols.length]);
        }
        TsDictColumn col = TsDictColumn.fromStrings(values);
        assertEquals(1000, col.size());
        assertEquals(4, col.cardinality());
        assertEquals(1000, col.codes().length);
    }

    @Test
    void equalStringsShareACode() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of("AAPL", "MSFT", "AAPL", "AAPL", "MSFT"));
        int[] codes = col.codes();
        assertEquals(codes[0], codes[2]);
        assertEquals(codes[0], codes[3]);
        assertEquals(codes[1], codes[4]);
        assertNotEquals(codes[0], codes[1]);
    }

    @Test
    void emptyColumn() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of());
        assertTrue(col.isEmpty());
        assertEquals(0, col.size());
        assertEquals(0, col.cardinality());
        assertEquals(Optional.empty(), col.decodeAt(0));
        assertTrue(col.toSeries().isEmpty());
        assertTrue(col.toList().isEmpty());
    }

    @Test
    void singleDistinctValue() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of("only", "only", "only"));
        assertEquals(3, col.size());
        assertEquals(1, col.cardinality());
        assertArrayEquals(new int[] {0, 0, 0}, col.codes());
        assertEquals(Optional.of("only"), col.lookup(0));
        assertEquals(Optional.empty(), col.lookup(1));
        assertEquals(Optional.empty(), col.lookup(-1));
    }

    @Test
    void lookupResolvesDictionaryCodes() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of("red", "green", "blue", "green"));
        assertEquals(Optional.of("red"), col.lookup(0));
        assertEquals(Optional.of("green"), col.lookup(1));
        assertEquals(Optional.of("blue"), col.lookup(2));
        assertEquals(Optional.empty(), col.lookup(3));
    }

    @Test
    void codesAreDefensivelyCopied() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of("a", "b"));
        int[] codes = col.codes();
        codes[0] = 99;
        assertArrayEquals(new int[] {0, 1}, col.codes());
    }

    // ---------- bridge to TsColumn ----------

    @Test
    void encodeColumnBridgesAStringColumn() {
        TsSeries<String> s = new TsSeries<>();
        int i = 0;
        for (String v : List.of("EU", "US", "EU")) {
            s.push(i++, v);
        }
        TsColumn col = new TsColumn.Str(s);
        TsDictColumn dict = TsDictColumn.encodeColumn(col).orElseThrow();
        assertEquals(2, dict.cardinality());
        assertArrayEquals(new int[] {0, 1, 0}, dict.codes());
    }

    @Test
    void encodeColumnRejectsNonStringColumn() {
        TsSeriesD s = new TsSeriesD();
        s.push(0, 1.0);
        TsColumn col = new TsColumn.F64(s);
        assertTrue(TsDictColumn.encodeColumn(col).isEmpty());
    }

    @Test
    void toListMatchesDecodeStream() {
        TsDictColumn col = TsDictColumn.fromStrings(List.of("p", "q", "p"));
        List<String> viaDecode = new ArrayList<>();
        for (int i = 0; i < col.size(); i++) {
            viaDecode.add(col.decodeAt(i).orElseThrow());
        }
        assertEquals(viaDecode, col.toList());
        // sanity on the stream form too.
        assertEquals(List.of("p", "q", "p"),
                col.toSeries().iterator().hasNext()
                        ? streamValues(col)
                        : List.of());
    }

    private static List<String> streamValues(TsDictColumn col) {
        List<String> out = new ArrayList<>();
        for (TsPoint<String> p : col.toSeries()) {
            out.add(p.value());
        }
        return out.stream().collect(Collectors.toList());
    }
}
