package com.submillisecond.recipes.tsinfluxdb;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TsInfluxdbTest {

    private static TsSeriesMetadata tagged(String name, long id, String... kv) {
        TsSeriesMetadata m = new TsSeriesMetadata(id, name);
        for (int i = 0; i < kv.length; i += 2) {
            m = m.withTag(kv[i], kv[i + 1]);
        }
        return m;
    }

    private static final String CSV =
            "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
            + ",result,table,_time,_value,_field,_measurement,host\n"
            + ",_result,0,2026-05-31T14:00:00Z,0.42,v,cpu,edge-01\n"
            + ",_result,0,2026-05-31T14:00:01Z,0.55,v,cpu,edge-01\n";

    @Test
    void rfc3339RoundtripWholeSecond() {
        long ts = Rfc3339.parseNanos("2026-05-31T14:00:00Z").getAsLong();
        assertEquals("2026-05-31T14:00:00Z", Rfc3339.formatNanos(ts));
    }

    @Test
    void rfc3339RoundtripWithFraction() {
        long ts = Rfc3339.parseNanos("2026-05-31T14:00:00.123456789Z").getAsLong();
        assertEquals("2026-05-31T14:00:00.123456789Z", Rfc3339.formatNanos(ts));
    }

    @Test
    void rfc3339ShortFraction() {
        long a = Rfc3339.parseNanos("2026-01-01T00:00:00.5Z").getAsLong();
        long base = Rfc3339.parseNanos("2026-01-01T00:00:00Z").getAsLong();
        assertEquals(base + 500_000_000L, a);
    }

    @Test
    void rfc3339RejectsGarbage() {
        assertTrue(Rfc3339.parseNanos("not-a-time").isEmpty());
        assertTrue(Rfc3339.parseNanos("2026-13-01T00:00:00Z").isEmpty());
        assertTrue(Rfc3339.parseNanos("2026-05-31T14:00:00").isEmpty());
    }

    @Test
    void encodeLineEscapesSpecials() {
        StringBuilder out = new StringBuilder();
        LineProtocol.encodeLine(
                "cpu load", List.of(Map.entry("data center", "us east,1")), 1.5, 42, out);
        assertEquals("cpu\\ load,data\\ center=us\\ east\\,1 v=1.5 42", out.toString());
    }

    @Test
    void encodeLineIntegerValueKeepsDecimal() {
        StringBuilder out = new StringBuilder();
        LineProtocol.encodeLine("m", List.of(), 100.0, 7, out);
        assertEquals("m v=100.0 7", out.toString());
    }

    @Test
    void encodeSeriesUsesMetadata() {
        TsSeriesD s = new TsSeriesD();
        s.push(10, 0.5);
        s.push(20, 0.75);
        s = s.withMetadata(tagged("cpu", 1, "host", "a", "region", "eu"));
        assertEquals(
                "cpu,host=a,region=eu v=0.5 10\ncpu,host=a,region=eu v=0.75 20",
                LineProtocol.encodeSeries(s, ""));
    }

    @Test
    void encodeSeriesMeasurementOverride() {
        TsSeriesD s = new TsSeriesD();
        s.push(10, 1.0);
        s = s.withMetadata(tagged("ignored", 1));
        assertTrue(LineProtocol.encodeSeries(s, "explicit").startsWith("explicit v=1.0 10"));
    }

    @Test
    void encodeCollectionOneBlockPerSeries() {
        TsCollection<Double> coll = new TsCollection<>();
        long a = coll.register(tagged("cpu", 1, "host", "a"));
        long b = coll.register(tagged("mem", 2, "host", "b"));
        coll.push(a, 1, 0.1);
        coll.push(b, 1, 0.2);
        String body = LineProtocol.encodeCollection(coll);
        assertTrue(body.contains("cpu,host=a v=0.1 1"));
        assertTrue(body.contains("mem,host=b v=0.2 1"));
        assertEquals(2, body.lines().count());
    }

    @Test
    void decodeBasicSingleSeries() {
        TsCollection<Double> coll = FluxCsv.decodeResponse(CSV);
        assertEquals(1, coll.size());
        TsSeries<Double> s = coll.byName("cpu,host=edge-01").orElseThrow();
        assertEquals(2, s.size());
        assertEquals(0.42, s.first().orElseThrow().value());
        assertEquals(0.55, s.last().orElseThrow().value());
    }

    @Test
    void decodeReconstructsTagsAndMultipleSeries() {
        String csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
                + ",result,table,_time,_value,_field,_measurement,host\n"
                + ",_result,0,2026-05-31T14:00:00Z,1.0,v,cpu,a\n"
                + ",_result,1,2026-05-31T14:00:00Z,2.0,v,cpu,b\n";
        TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
        assertEquals(2, coll.size());
        TsSeries<Double> a = coll.byTag("host", "a").get(0);
        assertEquals(1.0, a.first().orElseThrow().value());
    }

    @Test
    void decodeHandlesQuotedFields() {
        String csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
                + ",result,table,_time,_value,_field,_measurement,host\n"
                + ",_result,0,2026-05-31T14:00:00Z,3.5,v,\"cpu,total\",\"east, dc\"\n";
        TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
        assertEquals(1, coll.size());
        assertFalse(coll.byTag("host", "east, dc").isEmpty());
    }

    @Test
    void decodeOrdersPointsByTime() {
        String csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
                + ",result,table,_time,_value,_field,_measurement,host\n"
                + ",_result,0,2026-05-31T14:00:02Z,2.0,v,cpu,a\n"
                + ",_result,0,2026-05-31T14:00:00Z,0.0,v,cpu,a\n"
                + ",_result,0,2026-05-31T14:00:01Z,1.0,v,cpu,a\n";
        TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
        TsSeries<Double> s = coll.byName("cpu,host=a").orElseThrow();
        assertEquals(0.0, s.first().orElseThrow().value());
        assertEquals(2.0, s.last().orElseThrow().value());
    }

    @Test
    void decodeRejectsResponseWithoutTimeValue() {
        TsInfluxException e =
                assertThrows(TsInfluxException.class, () -> FluxCsv.decodeResponse("a,b\n1,2\n"));
        assertEquals(TsInfluxException.Kind.CSV, e.kind());
    }

    @Test
    void adapterWriteSeriesBuildsRequest() {
        CaptureTransport cap = CaptureTransport.ok("");
        TsSeriesD s = new TsSeriesD();
        s.push(10, 0.5);
        s = s.withMetadata(tagged("cpu", 1, "host", "a"));
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "tok", "myorg", "mybucket");
        assertEquals(1, a.writeSeries(s, ""));
    }

    @Test
    void adapterWriteRequestHasPathHeadersBody() {
        CaptureTransport cap = CaptureTransport.ok("");
        TsSeriesD s = new TsSeriesD();
        s.push(10, 0.5);
        s = s.withMetadata(tagged("cpu", 1, "host", "a"));
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "secrettok", "my org", "b");
        a.writeSeries(s, "");

        TsHttpRequest req = cap.last().orElseThrow();
        assertEquals("POST", req.method());
        assertTrue(req.path().startsWith("/api/v2/write?"));
        assertTrue(req.path().contains("precision=ns"));
        assertTrue(req.path().contains("org=my%20org"));
        assertTrue(req.path().contains("bucket=b"));
        assertTrue(req.headers().stream()
                .anyMatch(h -> h.name().equals("Authorization") && h.value().equals("Token secrettok")));
        assertEquals("cpu,host=a v=0.5 10", req.body());
    }

    @Test
    void adapterWriteEmptySeriesIsNoop() {
        CaptureTransport cap = CaptureTransport.ok("");
        TsSeriesD s = new TsSeriesD().withMetadata(tagged("cpu", 1));
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "t", "o", "b");
        assertEquals(0, a.writeSeries(s, ""));
    }

    @Test
    void adapterWriteCollection() {
        CaptureTransport cap = CaptureTransport.ok("");
        TsCollection<Double> coll = new TsCollection<>();
        long a = coll.register(tagged("cpu", 1, "host", "a"));
        coll.push(a, 1, 0.1);
        coll.push(a, 2, 0.2);
        TsInfluxAdapter adapter = TsInfluxAdapter.withTransport(cap, "t", "o", "b");
        assertEquals(2, adapter.writeCollection(coll));
    }

    @Test
    void adapterQueryFluxDecodes() {
        CaptureTransport cap = CaptureTransport.ok(CSV);
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "t", "o", "b");
        TsCollection<Double> coll = a.queryFlux("from(bucket:\"b\")");
        assertEquals(1, coll.size());
        assertEquals(2, coll.byName("cpu,host=edge-01").orElseThrow().size());
    }

    @Test
    void adapterQueryRequestUsesFluxContentType() {
        CaptureTransport cap = CaptureTransport.ok(CSV);
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "tok", "o", "b");
        a.queryFlux("from(bucket:\"b\")");
        TsHttpRequest req = cap.last().orElseThrow();
        assertTrue(req.path().startsWith("/api/v2/query?"));
        assertTrue(req.headers().stream()
                .anyMatch(h -> h.name().equals("Content-Type") && h.value().equals("application/vnd.flux")));
        assertEquals("from(bucket:\"b\")", req.body());
    }

    @Test
    void adapterHttpErrorSurfaces() {
        CaptureTransport cap = new CaptureTransport(List.of(new TsHttpResponse(500, "boom")));
        TsSeriesD s = new TsSeriesD();
        s.push(10, 0.5);
        s = s.withMetadata(tagged("cpu", 1));
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "t", "o", "b");
        TsSeriesD fs = s;
        TsInfluxException e = assertThrows(TsInfluxException.class, () -> a.writeSeries(fs, ""));
        assertEquals(TsInfluxException.Kind.HTTP, e.kind());
        assertEquals(500, e.status());
    }

    @Test
    void encodeLineNegativeAndFractionalValues() {
        StringBuilder out = new StringBuilder();
        LineProtocol.encodeLine("m", List.of(), -2.0, 1, out);
        assertEquals("m v=-2.0 1", out.toString());
        out.setLength(0);
        LineProtocol.encodeLine("m", List.of(), -0.25, 1, out);
        assertEquals("m v=-0.25 1", out.toString());
    }

    @Test
    void adapterWritesGenericDoubleSeries() {
        CaptureTransport cap = CaptureTransport.ok("");
        TsSeries<Double> s = new TsSeries<>();
        s.push(10, 0.5);
        s = s.withMetadata(tagged("cpu", 1, "host", "a"));
        TsInfluxAdapter a = TsInfluxAdapter.withTransport(cap, "t", "o", "b");
        assertEquals(1, a.writeSeries(s, ""));
        assertEquals("cpu,host=a v=0.5 10", cap.last().orElseThrow().body());
    }

    @Test
    void decodeWithoutMeasurementColumn() {
        String csv = "#datatype,string,long,dateTime:RFC3339,double,string\n"
                + ",result,table,_time,_value,_field\n"
                + ",_result,0,2026-05-31T14:00:00Z,7.0,v\n";
        TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
        assertEquals(1, coll.size());
    }

    @Test
    void connectRejectsNonHttp() {
        TsInfluxException e = assertThrows(
                TsInfluxException.class, () -> TsInfluxAdapter.connect("ftp://h", "t", "o", "b"));
        assertEquals(TsInfluxException.Kind.CONFIG, e.kind());
    }

    @Test
    void connectBuildsAdapter() {
        assertNotNull(TsInfluxAdapter.connect("http://localhost:8086", "t", "o", "b"));
        assertNotNull(TsInfluxAdapter.connect("https://localhost:8086", "t", "o", "b"));
    }
}
