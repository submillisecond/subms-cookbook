package com.submillisecond.otel;

import com.submillisecond.otel.autoconfig.SubMsOtelAutoConfig;
import com.submillisecond.otel.exemplars.Exemplar;
import com.submillisecond.otel.exemplars.ExemplarReservoir;
import com.submillisecond.otel.exporters.ExporterOtlpHelper;
import com.submillisecond.otel.exporters.PrometheusTextExporter;
import com.submillisecond.otel.state.OtelEventBridge;
import com.submillisecond.otel.state.StateTransitionRecorder;
import com.submillisecond.otel.testing.InMemorySpanExporter;
import com.submillisecond.otel.tracing.TracingObserver;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventDispatcher;
import com.submillisecond.recipes.events.EventLevel;

import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SimpleSpanProcessor;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    private static SdkMeterProvider prometheusProvider(PrometheusTextExporter exporter) {
        return SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofSeconds(60))
                        .build())
                .build();
    }

    @Test
    void quickstart() {
        // quickstart:begin
        List<String> seen = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addBridge(new OtelEventBridge());          // forwards each event to OpenTelemetry
        bus.addListener(e -> seen.add(e.topic()));
        bus.emit(Event.transition("gateway.health", EventLevel.WARN, "gateway", "UP", "DEGRADED"));
        assertEquals(List.of("gateway.health"), seen);
        // quickstart:end
    }

    @Test
    void baseEventsAndTransitionsReachOtel() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            Meter meter = provider.get("test");
            StateTransitionRecorder health =
                    new StateTransitionRecorder(meter, "gateway.health.transitions", "flips");
            EventDispatcher bus = EventDispatcher.sync();
            bus.addBridge(new OtelEventBridge(meter));

            bus.emit(Event.transition("gateway.health", EventLevel.WARN, "gateway", "UP", "DEGRADED"));
            bus.emit(Event.builder("order.rejected").build());
            bus.emit(Event.transition("gateway.health", EventLevel.INFO, "gateway", "DEGRADED", "UP"));
            health.record("gateway", "UP", "DEGRADED", Map.of("reason", "spread"));
            health.record("gateway", "DEGRADED", "UP");

            provider.forceFlush().join(5, TimeUnit.SECONDS);
            String text = exporter.scrape();
            assertTrue(text.contains("subms_events_total"), "events reach subms.events.total");
            assertTrue(text.contains("gateway_health_transitions_total"), "flips reach the transition counter");
        }
    }

    @Test
    void observerEmitsOnePerRecord() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            OtelObserver observer = new OtelObserver(provider.get("test"));
            SubMsObservationCtx ctx =
                    new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
            for (long ns : new long[] {120, 180, 240, 160, 900}) {
                observer.onRecord(ctx, ns);
            }
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            String text = exporter.scrape();
            assertTrue(text.contains("subms_bench_ops_total"), "ops_total emitted");
            assertTrue(text.contains("subms_latency_seconds"), "latency histogram emitted");
        }
    }

    @Test
    void reservoirKeepsSlowestInBucket() {
        ExemplarReservoir reservoir = new ExemplarReservoir(3);
        SubMsObservationCtx ctx =
                new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
        for (long ns : new long[] {600, 950, 700, 900, 800}) {
            reservoir.offer(ctx, ns);
        }
        List<Long> kept = new ArrayList<>();
        for (Exemplar ex : reservoir.snapshot()) kept.add(ex.ns());
        kept.sort(Long::compareTo);
        assertEquals(List.of(800L, 900L, 950L), kept, "slowest three in the shared bucket");
    }

    @Test
    void tracingEmitsOneSpanPerRecord() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        try (SdkTracerProvider provider = SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(exporter))
                .build()) {
            Tracer tracer = provider.get("test");
            TracingObserver observer = new TracingObserver(tracer);
            SubMsObservationCtx ctx =
                    new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
            observer.onRecord(ctx, 240);
            observer.onRecord(ctx, 310);

            List<SpanData> spans = exporter.getFinishedSpanItems();
            assertEquals(2, spans.size(), "one span per record");
            assertTrue(spans.stream().allMatch(s -> s.getName().equals(TracingObserver.TRACING_SPAN_NAME)));
        }
    }

    @Test
    void autoconfigWiresProvidersAndObserver() {
        SubMsOtelAutoConfig cfg = SubMsOtel.autoConfigure();
        try {
            assertNotNull(cfg.meter());
            assertNotNull(cfg.observer());
            assertNotNull(cfg.meterProvider());
        } finally {
            if (cfg.observer() instanceof AutoCloseable c) {
                try {
                    c.close();
                } catch (Exception ignored) {
                    // best-effort
                }
            }
            cfg.meterProvider().shutdown();
            cfg.tracerProvider().shutdown();
        }
    }

    @Test
    void otlpHelperBuildsProviders() {
        assertEquals(ExporterOtlpHelper.Protocol.GRPC, ExporterOtlpHelper.Protocol.fromEnv("grpc"));
        assertEquals(ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, ExporterOtlpHelper.Protocol.fromEnv(null));
        ExporterOtlpHelper.Wired wired = ExporterOtlpHelper.build(
                "http://localhost:4318",
                ExporterOtlpHelper.Protocol.HTTP_PROTOBUF,
                io.opentelemetry.sdk.resources.Resource.getDefault());
        assertNotNull(wired.meterProvider());
        assertNotNull(wired.tracerProvider());
        wired.meterProvider().shutdown();
        wired.tracerProvider().shutdown();
    }

    @Test
    void prometheusScrapeCarriesTheHistogram() {
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            SubMsOtel.exportSummary(SampleApp.fakeSummary(), provider.get("test"));
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            String text = exporter.scrape();
            assertTrue(text.contains("subms_latency_seconds"), "histogram lands in Prometheus text");
            assertTrue(text.contains("subms_stage=\"submit\""), "stage becomes a label");
        }
    }
}
