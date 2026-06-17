package com.submillisecond.recipes.tscsv;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.ts.TsDataType;

class TsNdjsonTest {

    private static TsColumn col(TsDataFrame df, String name) {
        return df.column(name).orElseThrow();
    }

    @Test
    void objectPerLine() {
        String text = "{\"a\":1,\"b\":1.5}\n{\"a\":2,\"b\":2.5}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals(TsDataType.I64, col(df, "a").dataType());
        assertEquals(TsDataType.F64, col(df, "b").dataType());
        assertEquals(2L, col(df, "a").asI64().orElseThrow().getAt(1).orElseThrow().value());
    }

    @Test
    void missingKeyIsGap() {
        String text = "{\"a\":1,\"b\":10}\n{\"a\":2}\n{\"a\":3,\"b\":30}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        TsColumn b = col(df, "b");
        assertEquals(2, b.len());
        assertTrue(b.get(1).isEmpty());
    }

    @Test
    void quotedValueStaysStr() {
        String text = "{\"id\":\"1\"}\n{\"id\":\"2\"}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        TsColumn id = col(df, "id");
        assertEquals(TsDataType.STR, id.dataType());
        assertEquals("1", id.asStr().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void boolAndNull() {
        String text = "{\"ok\":true,\"v\":1}\n{\"ok\":false,\"v\":null}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals(TsDataType.BOOL, col(df, "ok").dataType());
        assertFalse(col(df, "ok").asBool().orElseThrow().getAt(1).orElseThrow().value());
        assertEquals(1, col(df, "v").len()); // null on line 2 is a gap
    }

    @Test
    void tsColumn() {
        String text = "{\"t\":1000,\"v\":5}\n{\"t\":2000,\"v\":6}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults().tsColumn("t"));
        assertTrue(df.column("t").isEmpty());
        assertEquals(6L, col(df, "v").asI64().orElseThrow().getAt(2000).orElseThrow().value());
    }

    @Test
    void nestedObjectThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\":{\"nested\":1}}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void stringEscapes() {
        String text = "{\"s\":\"a\\tb\\n\"}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals("a\tb\n", col(df, "s").asStr().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void unicodeEscape() {
        String text = "{\"s\":\"\\u0041\"}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals("A", col(df, "s").asStr().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void blankLinesSkipped() {
        String text = "{\"a\":1}\n\n{\"a\":2}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals(2, col(df, "a").len());
    }

    @Test
    void malformedThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\" 1}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void notAnObjectThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("[1,2,3]\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void emptyObjectIsEmptyFrame() {
        TsDataFrame df = TsNdjson.readNdjson("{}\n", TsCsvOptions.defaults());
        assertTrue(df.isEmpty());
    }

    @Test
    void lateColumnDiscovery() {
        // `c` first appears on line 2; lines before it are gaps for `c`.
        String text = "{\"a\":1}\n{\"a\":2,\"c\":9}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals(2, col(df, "a").len());
        assertEquals(1, col(df, "c").len());
    }

    @Test
    void allJsonStringEscapes() {
        String text = "{\"s\":\"q\\\"bs\\\\sl\\/cr\\rbk\\bff\\f\"}\n";
        TsDataFrame df = TsNdjson.readNdjson(text, TsCsvOptions.defaults());
        assertEquals("q\"bs\\sl/cr\rbk\bff\f",
                col(df, "s").asStr().orElseThrow().getAt(0).orElseThrow().value());
    }

    @Test
    void badEscapeThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"s\":\"a\\x\"}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void unterminatedStringThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"s\":\"abc}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void badUnicodeEscapeThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"s\":\"\\uZZZZ\"}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void missingColonThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\" 1}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void missingValueThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\":}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void nestedArrayValueThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\":[1,2]}\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void trailingContentAfterObjectThrows() {
        TsCsvException ex = assertThrows(TsCsvException.class,
                () -> TsNdjson.readNdjson("{\"a\":1} junk\n", TsCsvOptions.defaults()));
        assertEquals(TsCsvException.Kind.BAD_JSON, ex.kind());
    }

    @Test
    void manyRowsGrowColumnStorage() {
        // > 8 rows forces the TsColumnBuilder ts-array to grow.
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 20; i++) {
            sb.append("{\"n\":").append(i).append("}\n");
        }
        TsDataFrame df = TsNdjson.readNdjson(sb.toString(), TsCsvOptions.defaults());
        assertEquals(20, col(df, "n").len());
        assertEquals(19L, col(df, "n").asI64().orElseThrow().getAt(19).orElseThrow().value());
    }
}
