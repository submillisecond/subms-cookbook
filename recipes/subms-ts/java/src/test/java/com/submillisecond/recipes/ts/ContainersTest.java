package com.submillisecond.recipes.ts;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

import org.junit.jupiter.api.Test;

class ContainersTest {

    private static final TsNumeric<Double> D = TsNumeric.DOUBLE;

    // ---------- value types ----------

    @Test
    void ohlcSeriesTimeQueries() {
        TsSeries<Ohlc> s = new TsSeries<>();
        s.push(1, Ohlc.of(1.0, 2.0, 0.5, 1.5, 100.0));
        s.push(2, Ohlc.of(1.5, 2.5, 1.0, 2.0, 120.0));
        assertEquals(2, s.size());
        assertEquals(2.0, s.nearestBefore(2).orElseThrow().value().close());
        List<TsPoint<Double>> closes = new ArrayList<>();
        for (TsPoint<Ohlc> p : s) closes.add(new TsPoint<>(p.ts(), p.value().close()));
        TsSeries<Double> closeSeries = TsSeries.fromPoints(closes);
        assertEquals(2.0, closeSeries.max(D).orElseThrow());
    }

    @Test
    void ohlcRejectsNanField() {
        TsSeries<Ohlc> s = new TsSeries<>();
        assertThrows(TsException.class, () -> s.push(1, Ohlc.of(1.0, Double.NaN, 0.5, 1.5, 100.0)));
        assertEquals(0, s.size());
    }

    @Test
    void curveSeriesAndPresence() {
        TsSeries<Curve> s = new TsSeries<>();
        s.push(1, Curve.of(new double[] {1.0, 2.0, 5.0}, new double[] {0.01, 0.015, 0.02}));
        assertEquals(3, s.first().orElseThrow().value().values().length);
        assertFalse(Curve.of(new double[] {1.0}, new double[] {Double.NaN}).tsIsPresent());
    }

    @Test
    void surfacePresenceAndShape() {
        Surface ok = Surface.of(new double[] {1.0}, new double[] {2.0}, new double[][] {{0.2}});
        assertTrue(ok.tsIsPresent());
        Surface bad = Surface.of(new double[] {1.0}, new double[] {2.0}, new double[][] {{Double.NaN}});
        assertFalse(bad.tsIsPresent());
        TsSeries<Surface> s = new TsSeries<>();
        s.push(1, ok);
        assertEquals(1, s.size());
    }

    @Test
    void tsValueNullRejectedButNestedNullOk() {
        TsSeries<TsValue> s = new TsSeries<>();
        assertThrows(TsException.class, () -> s.push(1, TsValue.nullValue()));
        Map<String, TsValue> map = new TreeMap<>();
        map.put("a", TsValue.nullValue());
        s.push(1, new TsValue.MapVal(map));
        assertEquals(1, s.size());
    }

    // ---------- metadata ----------

    @Test
    void attrsNormaliseAndRejectNonAscii() {
        TsAttrs a = new TsAttrs();
        a.insert("  Bar-Interval ", "  1M ");
        assertEquals(Optional.of("1m"), a.get("bar-interval"));
        assertEquals(Optional.of("1m"), a.get("BAR-INTERVAL"));
        assertTrue(a.matches("Bar-Interval", "1m"));
        assertThrows(TsAttrs.TsAttrException.class, () -> a.insert("naïve", "x"));
    }

    @Test
    void metadataBuilderAndTagMatch() {
        TsSeriesMetadata m = TsSeriesMetadata.of(7, "close.aapl")
                .withTag("symbol", "aapl")
                .withTag("field", "close");
        assertEquals(7, m.id());
        Map<String, String> want = Map.of("symbol", "aapl");
        assertTrue(m.hasTags(want));
        assertFalse(m.hasTags(Map.of("symbol", "msft")));
    }

    // ---------- codec ----------

    @Test
    void jsonCodecRoundtripEpochNanos() {
        TsSeriesD s = new TsSeriesD();
        s.push(1_000, 1.5);
        s.push(2_000, 2.0);
        s.push(3_000, 2.5);
        TsJsonCodec codec = new TsJsonCodec();
        TsSeriesD back = codec.decode(codec.encode(s));
        assertEquals(3, back.size());
        assertEquals(2.0, back.getAt(2_000).orElseThrow().value());
        List<Long> ts = new ArrayList<>();
        for (TsPoint<Double> p : back.toList()) ts.add(p.ts());
        assertEquals(List.of(1_000L, 2_000L, 3_000L), ts);
    }

    @Test
    void jsonCodecMillisRoundtripAndIsoEncode() {
        TsSeriesD s = new TsSeriesD();
        s.push(1_000_000_000L, 1.0);
        TsJsonCodec millis = new TsJsonCodec().withStyle(TsTimestampStyle.EPOCH_MILLIS);
        TsSeriesD back = millis.decode(millis.encode(s));
        assertEquals(1_000_000_000L, back.first().orElseThrow().ts());

        TsJsonCodec iso = new TsJsonCodec().withStyle(TsTimestampStyle.ISO8601);
        String text = new String(iso.encode(s), java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(text.contains("1970-01-01T00:00:01.000000000Z"), "got: " + text);
        assertThrows(TsCodecException.class,
                () -> iso.decode(text.getBytes(java.nio.charset.StandardCharsets.UTF_8)));
    }

    @Test
    void jsonCodecIncludesNameFromMetadata() {
        TsSeriesD s = new TsSeriesD().withMetadata(TsSeriesMetadata.of(1, "trades.aapl"));
        String text = new String(new TsJsonCodec().encode(s), java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(text.contains("\"name\":\"trades.aapl\""));
    }

    // ---------- collection ----------

    @Test
    void collectionRegisterPushLookup() {
        TsCollection<Double> c = new TsCollection<>();
        long id = c.register(TsSeriesMetadata.of(1, "cpu").withTag("host", "a"));
        c.push(id, 10, 0.5);
        c.push(id, 20, 0.7);
        assertEquals(1, c.size());
        assertEquals(2, c.byName("cpu").orElseThrow().size());
        assertEquals(1, c.byTag("host", "a").size());
        assertEquals(0, c.byTag("host", "z").size());
        TsCollectionException e = assertThrows(TsCollectionException.class,
                () -> c.register(TsSeriesMetadata.of(1, "dup")));
        assertEquals(TsCollectionException.Kind.DUPLICATE_ID, e.kind());
        assertThrows(TsCollectionException.class, () -> c.push(99, 1, 1.0));
    }

    @Test
    void collectionAggregateAtByTag() {
        TsCollection<Double> c = new TsCollection<>();
        long[] ids = {1, 2, 3};
        String[] hosts = {"a", "a", "b"};
        double[] vals = {1.0, 3.0, 9.0};
        for (int i = 0; i < ids.length; i++) {
            c.register(TsSeriesMetadata.of(ids[i], "s" + ids[i]).withTag("host", hosts[i]));
            c.push(ids[i], 100, vals[i]);
        }
        assertEquals(Optional.of(4.0), c.aggregateAtByTag("host", "a", 1_000, TsAgg.SUM, D));
        assertEquals(Optional.of(3.0), c.aggregateAtByTag("host", "a", 1_000, TsAgg.MAX, D));
        assertEquals(Optional.of(3.0), c.aggregateAt(1_000, TsAgg.COUNT, D));
    }

    @Test
    void collectionDeleteAndEvictByTag() {
        TsCollection<Double> c = new TsCollection<>();
        long[] ids = {1, 2, 3};
        String[] envs = {"prod", "prod", "dev"};
        for (int i = 0; i < ids.length; i++) {
            c.register(TsSeriesMetadata.of(ids[i], "s" + ids[i]).withTag("env", envs[i]));
            c.push(ids[i], 1, 1.0);
        }
        assertEquals(2, c.deleteRangeByTag("env", "prod", 0, 10));
        List<TsSeries<Double>> evicted = c.evictByTag("env", "prod");
        assertEquals(2, evicted.size());
        assertEquals(1, c.size());
    }

    // ---------- panel (homogeneous) ----------

    @Test
    void panelSlotsGroupsAndAligned() {
        TsPanel<Double> f = new TsPanel<>(TsPanelMetadata.of("ohlcv.aapl.1m"));
        TsSeries<Double> open = new TsSeries<>();
        TsSeries<Double> close = new TsSeries<>();
        open.push(1, 10.0);
        open.push(3, 12.0);
        close.push(1, 10.5);
        close.push(2, 11.0);
        f.addSeries("open", open);
        f.addSeries("close", close);
        f.addGroup(new TsPanelGroup("price", List.of("open", "close")));

        assertEquals(2, f.size());
        assertEquals(List.of("open", "close"), f.slotNames());
        assertEquals(2, f.seriesInGroup("price").size());

        List<TsPanelAligned.Row<Double>> rows = f.aligned().toList();
        assertEquals(3, rows.size());
        assertEquals(1, rows.get(0).ts());
        assertEquals(List.of(Optional.of(10.0), Optional.of(10.5)), rows.get(0).values());
        assertEquals(List.of(Optional.empty(), Optional.of(11.0)), rows.get(1).values());
        assertEquals(List.of(Optional.of(12.0), Optional.empty()), rows.get(2).values());
    }

    @Test
    void panelDeleteAndDrop() {
        TsPanel<Double> f = new TsPanel<>(TsPanelMetadata.of("f"));
        TsSeries<Double> a = new TsSeries<>();
        a.push(1, 1.0);
        a.push(2, 2.0);
        f.addSeries("a", a);
        assertEquals(1, f.deleteRange("a", 1, 1));
        assertEquals(1, f.series("a").orElseThrow().size());
        assertTrue(f.drop("a").isPresent());
        assertTrue(f.isEmpty());
    }

    // ---------- dataframe (heterogeneous) ----------

    private static TsSeriesD priceSeries() {
        TsSeriesD s = new TsSeriesD();
        s.push(1, 100.0);
        s.push(2, 101.5);
        return s;
    }

    private static TsSeries<String> symbolSeries() {
        TsSeries<String> s = new TsSeries<>();
        s.push(1, "AAPL");
        s.push(2, "AAPL");
        return s;
    }

    private static TsSeriesL volumeSeries() {
        TsSeriesL s = new TsSeriesL();
        s.push(1, 500);
        s.push(2, 700);
        return s;
    }

    private static TsDataFrame mixedFrame() {
        return new TsDataFrame()
                .withColumn("price", new TsColumn.F64(priceSeries()))
                .withColumn("symbol", new TsColumn.Str(symbolSeries()))
                .withColumn("volume", new TsColumn.I64(volumeSeries()));
    }

    @Test
    void columnF64Variant() {
        TsColumn col = new TsColumn.F64(priceSeries());
        assertEquals(TsDataType.F64, col.dataType());
        assertEquals(2, col.len());
        assertFalse(col.isEmpty());
        assertTrue(col.asF64().isPresent());
        assertTrue(col.asI64().isEmpty());
        assertEquals(Optional.of(TsValue.ofDouble(101.5)), col.get(2));
        assertEquals(Optional.empty(), col.get(3));
    }

    @Test
    void columnI64Variant() {
        TsColumn col = new TsColumn.I64(volumeSeries());
        assertEquals(TsDataType.I64, col.dataType());
        assertEquals(2, col.len());
        assertTrue(col.asI64().isPresent());
        assertTrue(col.asF64().isEmpty());
        assertEquals(Optional.of(TsValue.ofLong(500)), col.get(1));
    }

    @Test
    void columnBoolVariant() {
        TsSeries<Boolean> s = new TsSeries<>();
        s.push(1, true);
        s.push(2, false);
        TsColumn col = new TsColumn.Bool(s);
        assertEquals(TsDataType.BOOL, col.dataType());
        assertEquals(2, col.len());
        assertTrue(col.asBool().isPresent());
        assertTrue(col.asStr().isEmpty());
        assertEquals(Optional.of(TsValue.ofBool(false)), col.get(2));
    }

    @Test
    void columnStrVariant() {
        TsColumn col = new TsColumn.Str(symbolSeries());
        assertEquals(TsDataType.STR, col.dataType());
        assertEquals(2, col.len());
        assertTrue(col.asStr().isPresent());
        assertTrue(col.asValue().isEmpty());
        assertEquals(Optional.of(TsValue.ofString("AAPL")), col.get(1));
    }

    @Test
    void columnValueVariant() {
        TsSeries<TsValue> s = new TsSeries<>();
        s.push(1, TsValue.ofLong(7));
        s.push(2, TsValue.ofString("x"));
        TsColumn col = new TsColumn.Value(s);
        assertEquals(TsDataType.VALUE, col.dataType());
        assertEquals(2, col.len());
        assertTrue(col.asValue().isPresent());
        assertTrue(col.asF64().isEmpty());
        assertEquals(Optional.of(TsValue.ofString("x")), col.get(2));
    }

    @Test
    void columnEmptyIsEmpty() {
        TsColumn col = new TsColumn.F64(new TsSeriesD());
        assertTrue(col.isEmpty());
        assertEquals(0, col.len());
        assertEquals(Optional.empty(), col.get(1));
    }

    @Test
    void dataframeBuildAndAccess() {
        TsDataFrame df = mixedFrame();
        assertEquals(3, df.ncols());
        assertFalse(df.isEmpty());
        assertEquals(List.of("price", "symbol", "volume"), df.columnNames());
        assertEquals(TsDataType.F64, df.column("price").orElseThrow().dataType());
        assertEquals(TsDataType.STR, df.column("symbol").orElseThrow().dataType());
        assertTrue(df.column("missing").isEmpty());
    }

    @Test
    void dataframeSchemaIsDerived() {
        TsFrameSchema schema = mixedFrame().schema();
        assertEquals(3, schema.fields().size());
        assertEquals("price", schema.fields().get(0).name());
        assertEquals(TsDataType.F64, schema.fields().get(0).dataType());
        assertEquals("volume", schema.fields().get(2).name());
        assertEquals(TsDataType.I64, schema.fields().get(2).dataType());
    }

    @Test
    void dataframeSelectProjects() {
        TsDataFrame proj = mixedFrame().select("volume", "price");
        assertEquals(List.of("volume", "price"), proj.columnNames());
        assertTrue(proj.column("symbol").isEmpty());
        assertEquals(Optional.of(TsValue.ofDouble(100.0)), proj.column("price").orElseThrow().get(1));
    }

    @Test
    void dataframeDropAndRename() {
        TsDataFrame df = mixedFrame();
        assertTrue(df.drop("symbol").isPresent());
        assertEquals(2, df.ncols());
        assertTrue(df.column("symbol").isEmpty());

        assertTrue(df.rename("price", "px"));
        assertTrue(df.column("px").isPresent());
        assertTrue(df.column("price").isEmpty());
        assertFalse(df.rename("px", "volume"));
        assertFalse(df.rename("nope", "whatever"));
    }

    @Test
    void dataframeDuplicateColumnRejected() {
        TsDataFrame df = new TsDataFrame();
        assertTrue(df.pushColumn("a", new TsColumn.F64(priceSeries())));
        assertFalse(df.pushColumn("a", new TsColumn.I64(new TsSeriesL())));
        assertEquals(1, df.ncols());
    }

    @Test
    void dataframeAlignedYieldsGapNulls() {
        TsSeriesD price = new TsSeriesD();
        price.push(1, 100.0);
        price.push(3, 102.0);
        TsSeries<String> symbol = new TsSeries<>();
        symbol.push(1, "AAPL");
        symbol.push(2, "AAPL");
        TsDataFrame df = new TsDataFrame()
                .withColumn("price", new TsColumn.F64(price))
                .withColumn("symbol", new TsColumn.Str(symbol));

        List<TsDataFrame.Row> rows = df.aligned();
        assertEquals(3, rows.size());
        assertEquals(1, rows.get(0).ts());
        assertEquals(
                List.of(Optional.of(TsValue.ofDouble(100.0)), Optional.of(TsValue.ofString("AAPL"))),
                rows.get(0).values());
        assertEquals(
                List.of(Optional.empty(), Optional.of(TsValue.ofString("AAPL"))),
                rows.get(1).values());
        assertEquals(
                List.of(Optional.of(TsValue.ofDouble(102.0)), Optional.empty()),
                rows.get(2).values());
    }

    @Test
    void dataframeEmpty() {
        TsDataFrame df = new TsDataFrame();
        assertTrue(df.isEmpty());
        assertEquals(0, df.ncols());
        assertTrue(df.aligned().isEmpty());
        assertEquals(0, df.schema().fields().size());
    }
}
