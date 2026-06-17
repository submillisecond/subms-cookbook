package com.submillisecond.recipes.ts;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.Map;
import java.util.Optional;

import org.junit.jupiter.api.Test;

class TypesAndContainersApiTest {

    private static final TsNumeric<Double> D = TsNumeric.DOUBLE;
    private static final TsNumeric<Long> L = TsNumeric.LONG;

    @Test
    void numericOperatorBundles() {
        assertEquals(0.0, D.zero());
        assertEquals(3.0, D.add(1.0, 2.0));
        assertEquals(2.5, D.toDouble(2.5));
        assertTrue(D.compare(1.0, 2.0) < 0);
        assertEquals(0L, L.zero());
        assertEquals(5L, L.add(2L, 3L));
        assertEquals(7.0, L.toDouble(7L));
        assertTrue(L.compare(3L, 2L) > 0);
    }

    @Test
    void tsValueVariantsAndPresence() {
        assertTrue(TsValue.ofLong(1).tsIsPresent());
        assertTrue(TsValue.ofDouble(1.5).tsIsPresent());
        assertTrue(TsValue.ofBool(true).tsIsPresent());
        assertTrue(TsValue.ofString("x").tsIsPresent());
        assertFalse(TsValue.nullValue().tsIsPresent());
        assertTrue(new TsValue.Bytes(new byte[] {1, 2}).tsIsPresent());
        assertTrue(new TsValue.Array(List.of(TsValue.ofLong(1))).tsIsPresent());
        assertEquals(new TsValue.Bytes(new byte[] {1}), new TsValue.Bytes(new byte[] {1}));
        assertNotEquals(new TsValue.Bytes(new byte[] {1}), new TsValue.Bytes(new byte[] {2}));
        assertEquals(new TsValue.Bytes(new byte[] {1}).hashCode(), new TsValue.Bytes(new byte[] {1}).hashCode());
    }

    @Test
    void ohlcCurveSurfaceAccessorsAndEquality() {
        Ohlc o = Ohlc.of(1, 2, 0.5, 1.5, 100);
        assertEquals(1.5, o.close());
        assertTrue(o.tsIsPresent());
        Curve c = Curve.of(new double[] {1, 2}, new double[] {3, 4});
        assertEquals(2, c.axis().length);
        assertEquals(c, Curve.of(new double[] {1, 2}, new double[] {3, 4}));
        assertEquals(c.hashCode(), Curve.of(new double[] {1, 2}, new double[] {3, 4}).hashCode());
        assertNotEquals(c, Curve.of(new double[] {1, 2}, new double[] {3, 5}));
        assertTrue(c.toString().contains("Curve"));
        Surface s = Surface.of(new double[] {1}, new double[] {2}, new double[][] {{0.1, 0.2}});
        assertEquals(1, s.axisX().length);
        assertEquals(1, s.axisY().length);
        assertEquals(2, s.values()[0].length);
        assertEquals(s, Surface.of(new double[] {1}, new double[] {2}, new double[][] {{0.1, 0.2}}));
        assertEquals(s.hashCode(), Surface.of(new double[] {1}, new double[] {2}, new double[][] {{0.1, 0.2}}).hashCode());
        assertNotEquals(s, Surface.of(new double[] {1}, new double[] {9}, new double[][] {{0.1, 0.2}}));
    }

    @Test
    void formatCodecNames() {
        assertEquals("json", TsFormat.JSON.codecName());
        assertEquals("cbor", TsFormat.CBOR.codecName());
        assertEquals("gorilla", TsFormat.GORILLA.codecName());
        assertEquals("yaml", TsFormat.YAML.codecName());
        assertEquals("gzip+json", TsFormat.GZIP_JSON.codecName());
        assertEquals("gzip+cbor", TsFormat.GZIP_CBOR.codecName());
        assertEquals("gzip+gorilla", TsFormat.GZIP_GORILLA.codecName());
        assertEquals("proto", TsFormat.custom("proto").codecName());
    }

    @Test
    void schemaAndNumericKind() {
        assertTrue(TsSchema.anonymous() instanceof TsSchema.Anonymous);
        TsSchema.Numeric n = (TsSchema.Numeric) TsSchema.numeric("ms", TsNumericKind.GAUGE);
        assertEquals(Optional.of("ms"), n.unit());
        assertEquals(TsNumericKind.GAUGE, n.kind());
        assertTrue(new TsSchema.Schemaless() instanceof TsSchema);
        assertEquals("Foo", ((TsSchema.Custom) new TsSchema.Custom("Foo")).typeName());
        assertEquals(3, TsNumericKind.values().length);
        assertEquals(TsNumericKind.RATE, TsNumericKind.valueOf("RATE"));
    }

    @Test
    void depKindsAndDeps() {
        TsDep d = TsDep.of(7, TsDepKind.DERIVED);
        assertEquals(7, d.seriesId());
        assertTrue(d.note().isEmpty());
        TsDep withNote = d.withNote("hi");
        assertEquals(Optional.of("hi"), withNote.note());
        assertEquals(TsDepKind.AGGREGATE, TsDepKind.Builtin.AGGREGATE);
        assertEquals("foo", ((TsDepKind.Custom) TsDepKind.custom("foo")).name());
        assertEquals(TsDepKind.ASOF_JOIN_LEFT, TsDepKind.Builtin.ASOF_JOIN_LEFT);
        assertEquals(TsDepKind.ASOF_JOIN_RIGHT, TsDepKind.Builtin.ASOF_JOIN_RIGHT);
        assertEquals(TsDepKind.COMPONENT, TsDepKind.Builtin.COMPONENT);
    }

    @Test
    void metadataFullSurface() {
        TsSeriesMetadata m = TsSeriesMetadata.of(1, "s")
                .withSchema(TsSchema.numeric("pct", TsNumericKind.GAUGE))
                .withFormat(TsFormat.JSON)
                .withTag("k", "v")
                .withDependency(TsDep.of(2, TsDepKind.DERIVED));
        assertEquals(1, m.id());
        assertEquals(Optional.of(TsFormat.JSON), m.format());
        assertTrue(m.schema() instanceof TsSchema.Numeric);
        assertEquals("v", m.tags().get("k"));
        assertEquals(1, m.dependencies().size());
        assertTrue(m.attributes().isEmpty());
    }

    @Test
    void attrsSurface() {
        TsAttrs a = new TsAttrs();
        assertTrue(a.isEmpty());
        a.insert("Region", "US-East");
        assertEquals(1, a.size());
        assertEquals(Map.of("region", "us-east"), a.asMap());
        assertEquals(Optional.of("us-east"), a.remove("region"));
        assertTrue(a.isEmpty());
        a.insert("k", "v");
        TsAttrs b = new TsAttrs();
        b.insert("k", "v");
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
    }

    @Test
    void tagsAlias() {
        TsTags t = new TsTags(Map.of("a", "1"));
        assertEquals("1", t.get("a"));
        TsTags t2 = new TsTags();
        t2.put("b", "2");
        assertEquals("2", t2.get("b"));
    }

    @Test
    void collectionMiscSurface() {
        TsCollection<Double> c = new TsCollection<>();
        assertTrue(c.isEmpty());
        long id = c.register(TsSeriesMetadata.of(1, "a").withTag("env", "prod"));
        c.register(TsSeriesMetadata.of(2, "b").withTag("env", "dev"));
        c.push(id, 10, 1.0);
        c.push(2, 10, 2.0);
        assertTrue(c.contains(1));
        assertEquals(2, c.ids().size());
        assertEquals(2, c.series().size());
        assertTrue(c.get(1).isPresent());
        // both series carry a single point (1.0 for id=1, 2.0 for id=2) at ts=10
        assertEquals(Optional.of(3.0), c.aggregateAt(100, TsAgg.SUM, D));
        assertEquals(Optional.of(1.0), c.aggregateAt(100, TsAgg.MIN, D));
        assertEquals(Optional.of(1.5), c.aggregateAt(100, TsAgg.MEAN, D));
        assertEquals(Optional.of(1.0), c.deleteAt(1, 10).map(TsPoint::value));
        assertEquals(0, c.deleteRange(99, 0, 100));
        assertEquals(1, c.deleteAtByTag("env", "dev", 10));
        // id=1 now empty; id=2 now empty. Re-seed id=1 with two points.
        c.push(id, 20, 5.0);
        c.push(id, 30, 6.0);
        assertEquals(1, c.truncateBefore(25));
        assertEquals(1, c.truncateAfter(10));
        List<TsSeries<Double>> ev = c.evictWhere(m -> m.name().equals("a"));
        assertEquals(1, ev.size());
        assertTrue(c.deregister(99).isEmpty());
        c.clear();
        assertTrue(c.isEmpty());
        assertTrue(c.aggregateAt(0, TsAgg.SUM, D).isEmpty());
        assertTrue(c.byName("missing").isEmpty());
    }

    @Test
    void panelMiscSurface() {
        TsPanel<Double> f = new TsPanel<>(TsPanelMetadata.of("panel"));
        assertEquals("panel", f.metadata().name());
        TsSeries<Double> a = new TsSeries<>();
        a.push(1, 1.0);
        a.push(2, 2.0);
        f.addSeries("a", a);
        TsSeries<Double> b = new TsSeries<>();
        b.push(1, 3.0);
        f.addSeries("b", b);
        // replace slot
        TsSeries<Double> a2 = new TsSeries<>();
        a2.push(5, 9.0);
        f.addSeries("a", a2);
        assertEquals(5, f.series("a").orElseThrow().first().orElseThrow().ts());
        f.addGroup(new TsPanelGroup("g", List.of("a", "b")));
        assertEquals(2, f.seriesInGroup("g").size());
        assertTrue(f.group("g").isPresent());
        // slot a holds ts=5; slot b holds ts=1. truncateBefore(2) drops b's point.
        assertEquals(1, f.truncateBefore(2));
        f.series("b").orElseThrow().push(10, 4.0);
        assertEquals(1, f.truncateAfter(8));
        assertEquals(Optional.of(9.0), f.deleteAt("a", 5).map(TsPoint::value));
        assertTrue(f.removeGroup("g").isPresent());
        assertTrue(f.removeGroup("g").isEmpty());
        assertEquals(0, f.deleteRange("missing", 0, 1));
        assertTrue(f.deleteAt("missing", 0).isEmpty());
        assertTrue(f.series("missing").isEmpty());
        int dropped = f.retainSlots((name, s) -> name.equals("a"));
        assertEquals(1, dropped);
        f.clear();
        assertTrue(f.isEmpty());
    }
}
