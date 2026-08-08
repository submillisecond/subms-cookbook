package com.submillisecond.recipes.otel;

import com.submillisecond.recipes.otel.autoconfig.SubMsOtelAutoConfig;
import com.submillisecond.recipes.otel.exemplars.Exemplar;
import com.submillisecond.recipes.otel.exemplars.ExemplarReservoir;
import com.submillisecond.recipes.otel.exporters.ExporterOtlpHelper;
import com.submillisecond.recipes.otel.exporters.ExporterStdoutHelper;
import com.submillisecond.recipes.otel.exporters.PrometheusTextExporter;
import com.submillisecond.recipes.otel.resource.SubMsOtelResource;
import com.submillisecond.recipes.otel.state.OtelEventBridge;
import com.submillisecond.recipes.otel.state.StateTransitionRecorder;
import com.submillisecond.recipes.otel.tracing.TracingObserver;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsStageSummary;
import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventDispatcher;
import com.submillisecond.recipes.events.EventLevel;

import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.resources.Resource;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SimpleSpanProcessor;
import io.opentelemetry.sdk.trace.export.SpanExporter;

import java.time.Duration;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * Sample app: a tour of {@code subms-otel}, the OpenTelemetry bridge. The
 * scenario is a trading gateway whose health flips + lifecycle events are
 * exported to an OpenTelemetry collector, with the adapter's per-record
 * observer overhead standing in as the sub-ms claim. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.otel.SampleApp}
 *
 * <ul>
 *   <li>base                - health/event forwarding via the always-on bridge
 *   <li>observer            - the live per-record histogram observer (the hot path)
 *   <li>observer-async      - the queued variant: enqueue on the hot path, emit off it
 *   <li>exemplars           - slow-sample retention per latency bucket
 *   <li>tracing             - one span per recorded op
 *   <li>autoconfig          - env-driven one-line wiring
 *   <li>exporter-otlp       - build an OTLP-exporting MeterProvider
 *   <li>exporter-prometheus - see the exact Prometheus scrape bytes
 *   <li>exporter-stdout     - JSON-per-metric to stdout for local debugging
 * </ul>
 */
public final class SampleApp {

    private SampleApp() {}

    public static void main(String[] args) {
        baseGatewayHealthEvents();
        observerHotPath();
        observerAsyncQueued();
        exemplarsSlowTail();
        tracingSpans();
        autoconfigWiring();
        exporterOtlp();
        exporterPrometheus();
        exporterStdout();
    }

    /** Build a MeterProvider backed by the in-tree Prometheus text exporter for readback. */
    static SdkMeterProvider prometheusProvider(PrometheusTextExporter exporter) {
        return SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofSeconds(60))
                        .build())
                .build();
    }

    /** Base bridge: a trading gateway forwards its health flips + lifecycle events to OTel. */
    static void baseGatewayHealthEvents() {
        System.out.println("== base: gateway health + events to OTel ==");
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            Meter meter = provider.get("subms-otel");
            StateTransitionRecorder health =
                    new StateTransitionRecorder(meter, "gateway.health.transitions", "gateway health flips");

            EventDispatcher bus = EventDispatcher.sync();
            bus.addBridge(new OtelEventBridge(meter));

            bus.emit(Event.transition("gateway.health", EventLevel.WARN, "gateway", "UP", "DEGRADED"));
            bus.emit(Event.builder("order.rejected").attr("venue", "XNAS").build());
            bus.emit(Event.transition("gateway.health", EventLevel.INFO, "gateway", "DEGRADED", "UP"));
            health.record("gateway", "UP", "DEGRADED", Map.of("reason", "spread_widened"));
            health.record("gateway", "DEGRADED", "UP");

            provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);
            String text = exporter.scrape();
            System.out.println("  events + health flips forwarded to OTel counters");
            require(text.contains("subms_events_total"), "events land on subms.events.total");
            require(text.contains("gateway_health_transitions_total"), "health flips land on the transition counter");
        }
    }

    /** observer: the live per-record path. Its per-record cost is the recipe's sub-ms claim. */
    static void observerHotPath() {
        System.out.println("\n== observer: live per-record histogram ==");
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            OtelObserver observer = new OtelObserver(provider.get("subms-otel"));
            SubMsObservationCtx ctx =
                    new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
            for (long ns : new long[] {120, 180, 240, 160, 900}) {
                observer.onRecord(ctx, ns);
            }
            provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);
            String text = exporter.scrape();
            System.out.println("  5 hot-path records emitted a histogram point each");
            require(text.contains("subms_bench_ops_total"), "each record bumps ops_total");
            require(text.contains("subms_latency_seconds"), "each record lands in the latency histogram");
        }
    }

    /**
     * Queued variant: the recorder enqueues and a drain thread emits, so a slow exporter never
     * reaches the hot path. A full queue sheds the newest sample and counts it rather than blocking
     * the recorder; at the default depth nothing is shed here.
     */
    static void observerAsyncQueued() {
        System.out.println("\n== observer-async: enqueue on the hot path, emit off it ==");
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            try (OtelObserverAsync observer = new OtelObserverAsync(provider.get("subms-otel"))) {
                SubMsObservationCtx ctx =
                        new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
                for (long ns : new long[] {120, 180, 240, 160, 900}) {
                    observer.onRecord(ctx, ns);
                }
                observer.drainNow();
                provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);
                System.out.println("  5 records enqueued, drained off the recorder thread, "
                        + observer.droppedCount() + " dropped");
                require(observer.droppedCount() == 0, "5 samples do not overflow the default queue");
                require(observer.queueDepth() == 0, "drainNow leaves nothing queued");
                require(exporter.scrape().contains("subms_latency_seconds"),
                        "drained samples land in the same histogram the sync observer writes");
            }
        }
    }

    /** exemplars: keep the slowest K samples per latency bucket. Directly inspectable. */
    static void exemplarsSlowTail() {
        System.out.println("\n== exemplars: slowest-K tail retention ==");
        ExemplarReservoir reservoir = new ExemplarReservoir(3);
        SubMsObservationCtx ctx =
                new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
        for (long ns : new long[] {600, 950, 700, 900, 800}) {
            reservoir.offer(ctx, ns);
        }
        List<Long> kept = new ArrayList<>();
        for (Exemplar ex : reservoir.snapshot()) kept.add(ex.ns());
        Collections.sort(kept);
        System.out.println("  kept slowest " + kept.size() + " of 5: " + kept);
        require(kept.equals(List.of(800L, 900L, 950L)), "slowest three in the bucket retained");
    }

    /** tracing: one span per recorded op, captured here in-memory to verify the count. */
    static void tracingSpans() {
        System.out.println("\n== tracing: one span per record ==");
        CapturingSpanExporter exporter = new CapturingSpanExporter();
        try (SdkTracerProvider provider = SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(exporter))
                .build()) {
            Tracer tracer = provider.get("subms-otel");
            TracingObserver observer = new TracingObserver(tracer);
            SubMsObservationCtx ctx =
                    new SubMsObservationCtx("gateway-submit", "java", "submit", SubMsStageKind.HOT_PATH);
            observer.onRecord(ctx, 240);
            observer.onRecord(ctx, 310);

            List<String> names = exporter.spanNames();
            System.out.println("  emitted " + names.size() + " spans named " + TracingObserver.TRACING_SPAN_NAME);
            require(names.size() == 2, "one span per record");
            require(names.stream().allMatch(TracingObserver.TRACING_SPAN_NAME::equals), "span name matches");
        }
    }

    /** autoconfig: env-driven one-line wiring. No OTEL_* env here, so it falls back to stdout. */
    static void autoconfigWiring() {
        System.out.println("\n== autoconfig: env-driven wiring ==");
        SubMsOtelAutoConfig cfg = SubMsOtel.autoConfigure();
        System.out.println("  auto_configure wired a meter + tracer + observer");
        require(cfg.meter() != null && cfg.observer() != null, "providers + observer wired");
        if (cfg.observer() instanceof AutoCloseable c) {
            try {
                c.close();
            } catch (Exception ignored) {
                // best-effort observer shutdown
            }
        }
        cfg.meterProvider().shutdown();
        cfg.tracerProvider().shutdown();
    }

    /** exporter-otlp: build an OTLP/HTTP-exporting MeterProvider (lazy; no live collector needed). */
    static void exporterOtlp() {
        System.out.println("\n== exporter-otlp: OTLP providers ==");
        require(ExporterOtlpHelper.Protocol.fromEnv("grpc") == ExporterOtlpHelper.Protocol.GRPC, "grpc parses");
        require(ExporterOtlpHelper.Protocol.fromEnv(null) == ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, "default http");
        ExporterOtlpHelper.Wired wired = ExporterOtlpHelper.build(
                "http://localhost:4318", ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, Resource.getDefault());
        System.out.println("  OTLP/HTTP MeterProvider + TracerProvider built");
        require(wired.meterProvider() != null && wired.tracerProvider() != null, "OTLP providers built");
        wired.meterProvider().shutdown();
        wired.tracerProvider().shutdown();
    }

    /** exporter-prometheus: the one section that shows the exact scrape bytes. */
    static void exporterPrometheus() {
        System.out.println("\n== exporter-prometheus: scrape bytes ==");
        PrometheusTextExporter exporter = new PrometheusTextExporter();
        try (SdkMeterProvider provider = prometheusProvider(exporter)) {
            SubMsOtel.exportSummary(fakeSummary(), provider.get("subms-otel"));
            provider.forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);
            String text = exporter.scrape();
            String first = text.isEmpty() ? "" : text.split("\n", 2)[0];
            System.out.println("  scrape() first line: " + first);
            require(text.contains("subms_latency_seconds"), "histogram lands in Prometheus text");
            require(text.contains("subms_stage=\"submit\""), "stage attribute becomes a label");
        }
    }

    /** exporter-stdout: the autoconfig fallback; emits one JSON line per metric to stdout. */
    static void exporterStdout() {
        System.out.println("\n== exporter-stdout: JSON per metric ==");
        Resource resource = SubMsOtelResource.detect();
        ExporterStdoutHelper.Wired wired = ExporterStdoutHelper.build(resource);
        Meter meter = wired.meterProvider().get("subms-otel");
        meter.counterBuilder("subms.demo.stdout").build().add(1L);
        System.out.println("  emitting one metric line below:");
        wired.meterProvider().forceFlush().join(5, java.util.concurrent.TimeUnit.SECONDS);
        wired.meterProvider().shutdown();
        wired.tracerProvider().shutdown();
    }

    static SubMsBenchSummary fakeSummary() {
        SubMsStageSummary submit = new SubMsStageSummary(
                "submit", 5, 160, 900, 2100, 2100, 320, 40,
                Optional.of(new long[] {120, 180, 240, 160, 900}));
        return new SubMsBenchSummary(
                "gateway-submit", "java", "2026-07-28T00:00:00Z",
                null, null,
                Map.of("entries", "50000"),
                Map.of("subms.recipe.slug", "subms-otel"),
                List.of(submit));
    }

    static void require(boolean cond, String message) {
        if (!cond) {
            throw new AssertionError(message);
        }
    }

    /** In-memory span exporter used only by this sample's tracing section. */
    static final class CapturingSpanExporter implements SpanExporter {
        private final List<String> names = Collections.synchronizedList(new ArrayList<>());

        List<String> spanNames() {
            synchronized (names) {
                return new ArrayList<>(names);
            }
        }

        @Override
        public CompletableResultCode export(Collection<SpanData> spans) {
            for (SpanData s : spans) names.add(s.getName());
            return CompletableResultCode.ofSuccess();
        }

        @Override
        public CompletableResultCode flush() {
            return CompletableResultCode.ofSuccess();
        }

        @Override
        public CompletableResultCode shutdown() {
            return CompletableResultCode.ofSuccess();
        }
    }
}
