package com.submillisecond.otel.exporters;

import io.opentelemetry.api.common.AttributeKey;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.metrics.DoubleCounter;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.DoubleUpDownCounter;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.LongUpDownCounter;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.InstrumentType;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.AggregationTemporality;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import org.junit.jupiter.api.Test;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.Collections;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class PrometheusTextExporterTest {

    private SdkMeterProvider build(PrometheusTextExporter exporter) {
        return SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
    }

    @Test
    void histogramRendersWithSecondsSuffix() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        DoubleHistogram h = meter.histogramBuilder("subms.latency").setUnit("s").build();
        h.record(0.0001);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_latency_seconds_bucket"), text);
        assertTrue(text.contains("subms_latency_seconds_count"));
        assertTrue(text.contains("subms_latency_seconds_sum"));
        assertTrue(text.contains("le=\"+Inf\""));
    }

    @Test
    void counterRendersWithTotalSuffix() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        LongCounter c = meter.counterBuilder("subms.bench.ops").build();
        c.add(7);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_bench_ops_total"), text);
        assertTrue(text.contains(" 7"));
    }

    @Test
    void scrapeStartsEmpty() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        assertEquals("", exporter.scrape());
    }

    @Test
    void dotsBecomeUnderscoresInLabels() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        DoubleHistogram h = meter.histogramBuilder("subms.latency").setUnit("s").build();
        h.record(0.000001, Attributes.of(
                AttributeKey.stringKey("subms.stage"), "put",
                AttributeKey.stringKey("subms.recipe.slug"), "subms-bloom-filter"));
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_stage=\"put\""), text);
        assertTrue(text.contains("subms_recipe_slug=\"subms-bloom-filter\""));
    }

    @Test
    void emptyExportYieldsEmptyScrape() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        assertEquals("", exporter.scrape());
    }

    @Test
    void sanitizeNameKeepsValidChars() {
        assertEquals("subms_latency_seconds", PrometheusTextExporter.histogramName("subms.latency", "s"));
        assertEquals("subms_bench_ops_total", PrometheusTextExporter.counterName("subms.bench.ops"));
        assertEquals("subms_bench_ops_total", PrometheusTextExporter.counterName("subms.bench.ops_total"));
        assertEquals("subms_my_metric", PrometheusTextExporter.sanitizeName("subms.my-metric"));
        assertEquals("a:b_c", PrometheusTextExporter.sanitizeName("a:b.c"));
        assertEquals("ABC123_x", PrometheusTextExporter.sanitizeLabelKey("ABC123.x"));
    }

    @Test
    void escapesLabelValuesAndHelp() {
        assertEquals("a\\\\b\\\"c\\nd", PrometheusTextExporter.escapeLabelValue("a\\b\"c\nd"));
        assertEquals("a\\\\b\\nc", PrometheusTextExporter.escapeHelp("a\\b\nc"));
        assertEquals("plain", PrometheusTextExporter.escapeLabelValue("plain"));
        assertEquals("plain", PrometheusTextExporter.escapeHelp("plain"));
    }

    @Test
    void formatDoubleHandlesEdgeCases() {
        assertEquals("NaN", PrometheusTextExporter.formatDouble(Double.NaN));
        assertEquals("+Inf", PrometheusTextExporter.formatDouble(Double.POSITIVE_INFINITY));
        assertEquals("-Inf", PrometheusTextExporter.formatDouble(Double.NEGATIVE_INFINITY));
        assertEquals("0", PrometheusTextExporter.formatDouble(0.0));
        assertEquals("3", PrometheusTextExporter.formatDouble(3.0));
        assertFalse(PrometheusTextExporter.formatDouble(0.1).isEmpty());
    }

    @Test
    void gaugeFromUpDownCounter() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        LongUpDownCounter g = meter.upDownCounterBuilder("subms.queue.depth").build();
        g.add(5);
        g.add(-2);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_queue_depth"), text);
        assertFalse(text.contains("subms_queue_depth_total"));
        assertTrue(text.contains("# TYPE subms_queue_depth gauge"));
    }

    @Test
    void doubleCounterRenders() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        DoubleCounter c = meter.counterBuilder("subms.bench.rate").ofDoubles().build();
        c.add(1.5);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_bench_rate_total"), text);
    }

    @Test
    void doubleUpDownCounterRendersAsGauge() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        DoubleUpDownCounter g = meter.upDownCounterBuilder("subms.weight").ofDoubles().build();
        g.add(2.5);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("# TYPE subms_weight gauge"), text);
    }

    @Test
    void longGaugeFromObservable() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        meter.gaugeBuilder("subms.observed").ofLongs().buildWithCallback(o -> o.record(42L));
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("# TYPE subms_observed gauge"), text);
        assertTrue(text.contains(" 42"));
    }

    @Test
    void doubleGaugeFromObservable() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        meter.gaugeBuilder("subms.ratio").buildWithCallback(o -> o.record(0.75));
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("# TYPE subms_ratio gauge"), text);
    }

    @Test
    void interfaceContractMethods() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        assertEquals(AggregationTemporality.CUMULATIVE,
                exporter.getAggregationTemporality(InstrumentType.COUNTER));
        assertTrue(exporter.flush().isDone());
        assertTrue(exporter.shutdown().isDone());
        assertEquals(0, exporter.export(Collections.emptyList()).join(1, TimeUnit.SECONDS).isSuccess() ? 0 : 1);
        assertNotNull(exporter.drainSnapshot());
    }

    @Test
    void httpServerServesScrapeEndpoint() throws Exception {
        int port;
        try (java.net.ServerSocket s = new java.net.ServerSocket(0)) {
            port = s.getLocalPort();
        }
        PrometheusHttpServer srv = PrometheusHttpServer.builder().setPort(port).setHost("127.0.0.1").build();
        try {
            assertEquals(port, srv.getPort());
            // Drive a tiny export so the buffer has content.
            SdkMeterProvider provider = SdkMeterProvider.builder()
                    .registerMetricReader(PeriodicMetricReader.builder(srv)
                            .setInterval(Duration.ofDays(1))
                            .build())
                    .build();
            Meter meter = provider.get("smoke");
            LongCounter c = meter.counterBuilder("subms.bench.ops").build();
            c.add(1);
            provider.forceFlush().join(5, TimeUnit.SECONDS);

            String body = readMetricsBody("http://127.0.0.1:" + port + "/metrics");
            assertTrue(body.contains("subms_bench_ops_total"), body);
            provider.close();
        } finally {
            srv.shutdown().join(5, TimeUnit.SECONDS);
        }
    }

    @Test
    void httpServerExporterDelegatesContract() {
        int port;
        try {
            try (java.net.ServerSocket s = new java.net.ServerSocket(0)) {
                port = s.getLocalPort();
            }
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
        PrometheusHttpServer srv = PrometheusHttpServer.builder().setPort(port).setHost("127.0.0.1").build();
        try {
            assertEquals(AggregationTemporality.CUMULATIVE,
                    srv.getAggregationTemporality(InstrumentType.COUNTER));
            assertTrue(srv.flush().isDone());
        } finally {
            srv.shutdown().join(5, TimeUnit.SECONDS);
        }
    }

    @Test
    void httpServerBindErrorRaisesIllegalStateException() {
        // Bind once, then try to bind the same port again to force IOException.
        int port;
        try (java.net.ServerSocket s = new java.net.ServerSocket(0)) {
            port = s.getLocalPort();
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
        PrometheusHttpServer srv = PrometheusHttpServer.builder().setPort(port).setHost("127.0.0.1").build();
        try {
            assertThrows(IllegalStateException.class,
                    () -> PrometheusHttpServer.builder().setPort(port).setHost("127.0.0.1").build());
        } finally {
            srv.shutdown().join(5, TimeUnit.SECONDS);
        }
    }

    @Test
    void doubleAndLongListLabelValues() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        SdkMeterProvider provider = build(exporter);
        Meter meter = provider.get("smoke");
        LongCounter c = meter.counterBuilder("subms.with.list").build();
        // Trigger list-attribute path via boolean array; AttributeKey handles list rendering.
        c.add(1, Attributes.of(
                AttributeKey.stringArrayKey("subms.tags"), java.util.List.of("a", "b"),
                AttributeKey.doubleKey("subms.weight"), 1.5,
                AttributeKey.longKey("subms.count"), 3L,
                AttributeKey.booleanKey("subms.flag"), true));
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        String text = exporter.scrape();
        assertTrue(text.contains("subms_tags=\"a,b\""), text);
        assertTrue(text.contains("subms_weight=\"1.5\""), text);
        assertTrue(text.contains("subms_flag=\"true\""), text);
        assertNotEquals(-1, text.indexOf("subms_count=\"3\""));
    }

    @Test
    void emptyExportInDelegateBufferStillProducesEmpty() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        assertDoesNotThrow(() -> exporter.export(Collections.emptyList()).join(1, TimeUnit.SECONDS));
        assertEquals("", exporter.scrape());
    }

    private static String readMetricsBody(String url) throws IOException {
        HttpURLConnection conn = (HttpURLConnection) URI.create(url).toURL().openConnection();
        conn.setRequestMethod("GET");
        try (BufferedReader r = new BufferedReader(new InputStreamReader(conn.getInputStream(), StandardCharsets.UTF_8))) {
            StringBuilder b = new StringBuilder();
            String line;
            while ((line = r.readLine()) != null) {
                b.append(line).append('\n');
            }
            return b.toString();
        }
    }
}
