package com.submillisecond.recipes.otel;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsStageSummary;
import com.submillisecond.perf.SubMsTimer;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import com.submillisecond.recipes.otel.testing.InMemoryMetricExporter;
import com.submillisecond.recipes.otel.testing.InMemorySpanExporter;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SimpleSpanProcessor;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class SubMsOtelTest {

    @Test
    void histogramBoundariesByKind() {
        List<Double> hot = SubMsOtel.histogramBoundaries(SubMsStageKind.HOT_PATH);
        assertEquals(12, hot.size());
        assertEquals(5e-8, hot.get(0));
        assertEquals(1e-3, hot.get(hot.size() - 1));

        List<Double> batch = SubMsOtel.histogramBoundaries(SubMsStageKind.BATCH_OP);
        List<Double> oneShot = SubMsOtel.histogramBoundaries(SubMsStageKind.ONE_SHOT);
        assertEquals(batch, oneShot);
        assertEquals(7, batch.size());

        assertTrue(SubMsOtel.histogramBoundaries(SubMsStageKind.UNSPECIFIED).isEmpty());

        assertThrows(NullPointerException.class, () -> SubMsOtel.histogramBoundaries(null));
    }

    @Test
    void exportSummaryEmitsSingleHistogramKeyedByStageAttribute() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-otel-test");

        SubMsBenchSummary summary = buildSummary(/*withSamples*/ true);
        SubMsOtel.exportSummary(summary, meter);
        provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);

        List<MetricData> metrics = exporter.getFinishedMetricItems();
        assertEquals(1, metrics.size(), "one shared histogram across stages");

        MetricData latency = findByName(metrics, "subms.latency");
        assertEquals("s", latency.getUnit());

        List<HistogramPointData> points = new ArrayList<>(latency.getHistogramData().getPoints());
        // Two distinct attribute sets (one per stage), one point each.
        assertEquals(2, points.size(), "one point per stage attribute set");

        HistogramPointData put = points.stream()
                .filter(p -> "put".equals(p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE)))
                .findFirst().orElseThrow();
        // 6 percentiles + 3 samples = 9 records.
        assertEquals(9L, put.getCount());

        // semantic-convention attribute set
        assertEquals("bloom-filter", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_WORKLOAD));
        assertEquals("java", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_LANG));
        assertEquals("put", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
        assertEquals("ci-runner", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_HOST));
        assertEquals("ci-shared", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_HARDWARE_TIER));
        assertEquals("0.5.1", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_CRATE_VERSION));
        assertEquals("subms-bloom-filter", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_RECIPE_SLUG));
        assertEquals("probabilistic", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_RECIPE_CATEGORY));
        assertEquals("50000", put.getAttributes().get(SubMsOtelAttributeKeys.KEY_WORKLOAD_ENTRIES));

        // Values land in seconds (ns / 1e9). With ns values 50..300 across 9 records, sum is in the low us.
        assertTrue(put.getSum() < 1e-5,
                "sum should be in seconds (ns / 1e9), got " + put.getSum());
        assertTrue(put.getSum() > 1e-9,
                "sum should be > 0 in seconds, got " + put.getSum());
    }

    @Test
    void exportSummaryHandlesLeanSummaryWithoutSamples() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-otel-test");

        SubMsBenchSummary summary = buildSummary(/*withSamples*/ false);
        SubMsOtel.exportSummary(summary, meter);
        provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);

        MetricData latency = findByName(exporter.getFinishedMetricItems(), "subms.latency");
        assertEquals("s", latency.getUnit());
        HistogramPointData put = latency.getHistogramData().getPoints().stream()
                .filter(p -> "put".equals(p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE)))
                .findFirst().orElseThrow();
        // Only 6 percentiles when samples missing.
        assertEquals(6L, put.getCount());
    }

    @Test
    void exportSummaryFallsBackToJarVersionWhenCrateMissing() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-otel-test");

        Map<String, String> meta = new LinkedHashMap<>();
        meta.put("jar_version", "0.5.0");
        SubMsBenchSummary summary = new SubMsBenchSummary(
                "wl", "java", "2026-05-30T12:00:00Z",
                null, null,
                Map.of(),
                meta,
                List.of(new SubMsStageSummary("put", 1, 10, 20, 30, 40, 25, 5, Optional.empty())));

        SubMsOtel.exportSummary(summary, meter);
        provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);

        HistogramPointData p = findByName(exporter.getFinishedMetricItems(), "subms.latency")
                .getHistogramData().getPoints().iterator().next();
        assertEquals("0.5.0", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_CRATE_VERSION));
    }

    @Test
    void exportSummaryRejectsNullArgs() {
        SdkMeterProvider provider = SdkMeterProvider.builder().build();
        Meter meter = provider.get("t");
        assertThrows(NullPointerException.class, () -> SubMsOtel.exportSummary(null, meter));
        assertThrows(NullPointerException.class, () -> SubMsOtel.exportSummary(buildSummary(true), null));
    }

    @Test
    void exportTimerEmitsParentAndChildSpans() {
        InMemorySpanExporter spans = InMemorySpanExporter.create();
        SdkTracerProvider tp = SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(spans))
                .build();
        Tracer tracer = tp.get("subms-otel-test");

        SubMsTimer t = new SubMsTimer("parse-request");
        t.mark("headers-read");
        t.mark("body-decoded");
        t.stop("served");

        SubMsOtel.exportTimer(t, tracer);
        tp.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);

        List<SpanData> emitted = spans.getFinishedSpanItems();
        // 1 parent + 3 children.
        assertEquals(4, emitted.size());

        SpanData parent = emitted.stream()
                .filter(s -> s.getName().equals("subms.timer.parse-request"))
                .findFirst().orElseThrow();
        assertNotNull(parent.getAttributes().get(io.opentelemetry.api.common.AttributeKey.longKey("subms.total.ns")));

        long children = emitted.stream()
                .filter(s -> !s.getName().equals("subms.timer.parse-request"))
                .count();
        assertEquals(3L, children);

        SpanData served = emitted.stream()
                .filter(s -> s.getName().equals("served"))
                .findFirst().orElseThrow();
        assertEquals(Boolean.TRUE,
                served.getAttributes().get(io.opentelemetry.api.common.AttributeKey.booleanKey("subms.is_stop")));
    }

    @Test
    void exportTimerHandlesUnnamedTimer() {
        InMemorySpanExporter spans = InMemorySpanExporter.create();
        SdkTracerProvider tp = SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(spans))
                .build();
        Tracer tracer = tp.get("subms-otel-test");

        SubMsTimer t = new SubMsTimer();
        t.stop("done");
        SubMsOtel.exportTimer(t, tracer);
        tp.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);

        assertTrue(spans.getFinishedSpanItems().stream()
                .anyMatch(s -> s.getName().equals("subms.timer.unnamed")));
    }

    @Test
    void exportTimerRejectsNullArgs() {
        SdkTracerProvider tp = SdkTracerProvider.builder().build();
        Tracer tracer = tp.get("t");
        assertThrows(NullPointerException.class, () -> SubMsOtel.exportTimer(null, tracer));
        assertThrows(NullPointerException.class, () -> SubMsOtel.exportTimer(new SubMsTimer("x"), null));
    }

    @Test
    void publicConstantsExposed() {
        // Histogram identity matches the Rust sibling.
        assertEquals("subms.latency", SubMsOtel.HISTOGRAM_NAME);
        assertEquals("s", SubMsOtel.HISTOGRAM_UNIT);
        // Attribute-key smoke: every public constant is non-null + non-empty.
        assertEquals("subms.workload", SubMsOtelAttributeKeys.WORKLOAD);
        assertEquals("subms.lang", SubMsOtelAttributeKeys.LANG);
        assertEquals("subms.stage", SubMsOtelAttributeKeys.STAGE);
        assertEquals("subms.stage.kind", SubMsOtelAttributeKeys.STAGE_KIND);
        assertNotNull(SubMsOtelAttributeKeys.KEY_WORKLOAD);
        assertNotNull(SubMsOtelAttributeKeys.KEY_LANG);
        assertNotNull(SubMsOtelAttributeKeys.KEY_STAGE);
        assertNotNull(SubMsOtelAttributeKeys.KEY_STAGE_KIND);
        assertNotEquals(SubMsOtelAttributeKeys.KEY_WORKLOAD, SubMsOtelAttributeKeys.KEY_LANG);
        assertFalse(SubMsOtelAttributeKeys.WORKLOAD.isEmpty());
    }

    static SubMsBenchSummary buildSummary(boolean withSamples) {
        Map<String, String> inputs = new LinkedHashMap<>();
        inputs.put("entries", "50000");
        inputs.put("seed", "0");

        Map<String, String> meta = new LinkedHashMap<>();
        meta.put("host", "ci-runner");
        meta.put("hardware_tier", "ci-shared");
        meta.put("crate_version", "0.5.1");
        meta.put(SubMsOtelAttributeKeys.RECIPE_SLUG, "subms-bloom-filter");
        meta.put(SubMsOtelAttributeKeys.RECIPE_CATEGORY, "probabilistic");
        meta.put(SubMsOtelAttributeKeys.WORKLOAD_FEATURE, "default");

        Optional<long[]> samples = withSamples
                ? Optional.of(new long[] {100, 200, 300})
                : Optional.empty();

        SubMsStageSummary put = new SubMsStageSummary(
                "put", 3, 100, 200, 250, 300, 200, 50, samples);
        SubMsStageSummary getHit = new SubMsStageSummary(
                "get_hit", 3, 110, 210, 260, 310, 210, 55, samples);

        return new SubMsBenchSummary(
                "bloom-filter", "java", "2026-05-30T12:00:00Z",
                null, null,
                inputs, meta, List.of(put, getHit));
    }

    static MetricData findByName(List<MetricData> metrics, String name) {
        return metrics.stream()
                .filter(m -> m.getName().equals(name))
                .findFirst()
                .orElseThrow(() -> new AssertionError("missing metric: " + name + " in " + metrics));
    }
}
