package com.submillisecond.recipes.tscsv;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;

class TsCsvTest {

    private static TsColumn col(TsDataFrame df, String name) {
        return df.column(name).orElseThrow();
    }

    // --- per-column type inference ---------------------------------------

    @Test
    void infersI64Column() {
        TsDataFrame df = TsCsv.readCsv("n\n1\n2\n3\n", TsCsvOptions.defaults());
        TsColumn c = col(df, "n");
        assertEquals(TsDataType.I64, c.dataType());
        assertEquals(3, c.len());
        assertEquals(1L, c.asI64().orElseThrow().getAt(0).orElseThrow().value());
        assertEquals(3L, c.asI64().orElseThrow().getAt(2).orElseThrow().value());
    }

    @Test
    void infersF64Column() {
        TsDataFrame df = TsCsv.readCsv("x\n1.5\n2\n3.25\n", TsCsvOptions.defaults());
        TsColumn c = col(df, "x");
        assertEquals(TsDataType.F64, c.dataType());
        assertEquals(1.5, c.asF64().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void infersBoolColumn() {
        TsDataFrame df = TsCsv.readCsv("ok\ntrue\nfalse\nTrue\n", TsCsvOptions.defaults());
        TsColumn c = col(df, "ok");
        assertEquals(TsDataType.BOOL, c.dataType());
        assertTrue(c.asBool().orElseThrow().getAt(0).orElseThrow().value());
        assertFalse(c.asBool().orElseThrow().getAt(1).orElseThrow().value());
        assertTrue(c.asBool().orElseThrow().getAt(2).orElseThrow().value());
    }

    @Test
    void infersStrColumn() {
        TsDataFrame df = TsCsv.readCsv("tag\nfoo\nbar\nbaz\n", TsCsvOptions.defaults());
        TsColumn c = col(df, "tag");
        assertEquals(TsDataType.STR, c.dataType());
        assertEquals("bar", c.asStr().orElseThrow().getAt(1).orElseThrow().value());
    }

    @Test
    void mixedIntAndTextInfersStr() {
        TsDataFrame df = TsCsv.readCsv("v\n1\n2\nNA\n4\n", TsCsvOptions.defaults());
        TsColumn c = col(df, "v");
        assertEquals(TsDataType.STR, c.dataType());
        assertEquals("1", c.asStr().orElseThrow().getAt(0).orElseThrow().value());
        assertEquals("NA", c.asStr().orElseThrow().getAt(2).orElseThrow().value());
    }

    // --- empty cell as gap -----------------------------------------------

    @Test
    void emptyCellIsAGap() {
        TsDataFrame df = TsCsv.readCsv("a,b\n1,10\n2,\n3,30\n", TsCsvOptions.defaults());
        TsColumn b = col(df, "b");
        assertEquals(TsDataType.I64, b.dataType());
        assertEquals(2, b.len()); // gap pushed no null
        assertTrue(b.get(1).isEmpty());

        List<TsDataFrame.Row> rows = df.aligned();
        TsDataFrame.Row r1 = rows.stream().filter(r -> r.ts() == 1).findFirst().orElseThrow();
        assertTrue(r1.values().get(0).isPresent()); // a
        assertTrue(r1.values().get(1).isEmpty());   // b gap
    }

    // --- quoting ----------------------------------------------------------

    @Test
    void quotedFieldWithCommaAndEscapedQuote() {
        String text = "name,note\n1,\"a,b\"\n2,\"he said \"\"hi\"\"\"\n";
        TsDataFrame df = TsCsv.readCsv(text, TsCsvOptions.defaults());
        TsColumn note = col(df, "note");
        assertEquals(TsDataType.STR, note.dataType());
        assertEquals("a,b", note.asStr().orElseThrow().getAt(0).orElseThrow().value());
        assertEquals("he said \"hi\"", note.asStr().orElseThrow().getAt(1).orElseThrow().value());
    }

    @Test
    void unterminatedQuoteThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsCsv.readCsv("a\n\"oops\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_QUOTING, ex.kind());
    }

    // --- line endings -----------------------------------------------------

    @Test
    void crlfAndLfLineEndings() {
        TsDataFrame crlf = TsCsv.readCsv("a,b\r\n1,2\r\n3,4\r\n", TsCsvOptions.defaults());
        TsDataFrame lf = TsCsv.readCsv("a,b\n1,2\n3,4\n", TsCsvOptions.defaults());
        assertEquals(2, col(crlf, "a").len());
        assertEquals(2, col(lf, "a").len());
        assertEquals(4L, col(crlf, "b").asI64().orElseThrow().getAt(1).orElseThrow().value());
    }

    // --- ts axis ----------------------------------------------------------

    @Test
    void tsColumnDesignation() {
        String text = "t,v\n100,1.0\n200,2.0\n300,3.0\n";
        TsDataFrame df = TsCsv.readCsv(text, TsCsvOptions.defaults().tsColumn("t"));
        assertTrue(df.column("t").isEmpty()); // consumed, not re-emitted
        var v = col(df, "v").asF64().orElseThrow();
        assertEquals(2.0, v.getAt(200).orElseThrow().value());
        assertEquals(100L, v.first().orElseThrow().ts());
        assertEquals(300L, v.last().orElseThrow().ts());
    }

    @Test
    void rowIndexDefaultAxis() {
        TsDataFrame df = TsCsv.readCsv("v\n10\n20\n30\n", TsCsvOptions.defaults());
        var v = col(df, "v").asI64().orElseThrow();
        assertEquals(10L, v.getAt(0).orElseThrow().value());
        assertEquals(30L, v.getAt(2).orElseThrow().value());
    }

    @Test
    void tsColumnUnknownThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsCsv.readCsv("a\n1\n", TsCsvOptions.defaults().tsColumn("nope")));
        assertEquals(TsCsvException.Kind.UNKNOWN_TS_COLUMN, ex.kind());
    }

    @Test
    void tsColumnNonIntegerThrows() {
        String text = "t,v\n100,1\nnotanint,2\n";
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsCsv.readCsv(text, TsCsvOptions.defaults().tsColumn("t")));
        assertEquals(TsCsvException.Kind.BAD_TIMESTAMP, ex.kind());
    }

    // --- header vs no header ---------------------------------------------

    @Test
    void noHeaderSynthesisesNames() {
        TsDataFrame df = TsCsv.readCsv("1,2,3\n4,5,6\n", TsCsvOptions.defaults().hasHeader(false));
        assertEquals(3, df.ncols());
        assertEquals(List.of("col0", "col1", "col2"), df.columnNames());
        assertEquals(2L, col(df, "col1").asI64().orElseThrow().getAt(0).orElseThrow().value());
    }

    // --- ragged rows ------------------------------------------------------

    @Test
    void raggedRowThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsCsv.readCsv("a,b,c\n1,2,3\n4,5\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.RAGGED_ROW, ex.kind());
    }

    // --- round trip -------------------------------------------------------

    @Test
    void roundTripPreservesColumnsTypesValues() {
        String text = "i,f,b,s\n1,1.5,true,foo\n2,2.5,false,\"a,b\"\n3,3.5,true,baz\n";
        TsDataFrame df = TsCsv.readCsv(text, TsCsvOptions.defaults());
        String emitted = TsCsv.writeCsv(df);
        TsDataFrame again = TsCsv.readCsv(emitted, TsCsvOptions.defaults());

        assertEquals(List.of("i", "f", "b", "s"), again.columnNames());
        assertEquals(TsDataType.I64, col(again, "i").dataType());
        assertEquals(TsDataType.F64, col(again, "f").dataType());
        assertEquals(TsDataType.BOOL, col(again, "b").dataType());
        assertEquals(TsDataType.STR, col(again, "s").dataType());

        assertEquals(2L, col(again, "i").asI64().orElseThrow().getAt(1).orElseThrow().value());
        assertEquals(3.5, col(again, "f").asF64().orElseThrow().getAt(2).orElseThrow().value());
        assertTrue(col(again, "b").asBool().orElseThrow().getAt(0).orElseThrow().value());
        assertEquals("a,b", col(again, "s").asStr().orElseThrow().getAt(1).orElseThrow().value());
    }

    @Test
    void writeQuotesOnlyWhenNeeded() {
        String text = "a,b\n1,plain\n2,\"has,comma\"\n";
        TsDataFrame df = TsCsv.readCsv(text, TsCsvOptions.defaults());
        String out = TsCsv.writeCsv(df);
        assertTrue(out.contains("plain"));
        assertTrue(out.contains("\"has,comma\""));
    }

    @Test
    void emptyInputIsEmptyFrame() {
        TsDataFrame df = TsCsv.readCsv("", TsCsvOptions.defaults());
        assertTrue(df.isEmpty());
    }

    @Test
    void customDelimiter() {
        TsDataFrame df = TsCsv.readCsv("a;b\n1;2\n", TsCsvOptions.defaults().delimiter(';'));
        assertEquals(2, df.ncols());
        assertEquals(2L, col(df, "b").asI64().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void scientificNotationInfersF64() {
        TsDataFrame df = TsCsv.readCsv("x\n1e3\n2.5E-1\n", TsCsvOptions.defaults());
        assertEquals(TsDataType.F64, col(df, "x").dataType());
        assertEquals(1000.0, col(df, "x").asF64().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void nonFiniteTokenFallsThroughToStr() {
        // "Infinity" / "NaN" parse as a double in Java but are not finite, so
        // the column must stay Str and preserve the literal.
        TsDataFrame df = TsCsv.readCsv("x\n1.0\nInfinity\n", TsCsvOptions.defaults());
        assertEquals(TsDataType.STR, col(df, "x").dataType());
        assertEquals("Infinity", col(df, "x").asStr().orElseThrow().getAt(1).orElseThrow().value());
    }

    @Test
    void textWithDigitsAndLetterInfersStr() {
        // a cell with a non-numeric char fails the cheap pre-scan, so the
        // column is Str without ever calling Double.parseDouble.
        TsDataFrame df = TsCsv.readCsv("x\n1a\n2b\n", TsCsvOptions.defaults());
        assertEquals(TsDataType.STR, col(df, "x").dataType());
    }

    @Test
    void manyRowsGrowColumnStorage() {
        StringBuilder sb = new StringBuilder("n\n");
        for (int i = 0; i < 50; i++) {
            sb.append(i).append('\n');
        }
        TsDataFrame df = TsCsv.readCsv(sb.toString(), TsCsvOptions.defaults());
        assertEquals(50, col(df, "n").len());
        assertEquals(49L, col(df, "n").asI64().orElseThrow().getAt(49).orElseThrow().value());
    }
}
