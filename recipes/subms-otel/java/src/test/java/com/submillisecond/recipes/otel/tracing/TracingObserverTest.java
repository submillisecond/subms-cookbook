package com.submillisecond.recipes.otel.tracing;

import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.trace.Span;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.context.Context;
import io.opentelemetry.context.Scope;
import com.submillisecond.recipes.otel.testing.InMemorySpanExporter;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SimpleSpanProcessor;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class TracingObserverTest {

    private SdkTracerProvider buildProvider(InMemorySpanExporter exporter) {
        return SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(exporter))
                .build();
    }

    @Test
    void onRecordEmitsNamedSpan() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        SdkTracerProvider provider = buildProvider(exporter);
        Tracer tracer = provider.get("subms-otel-tracing-test");
        TracingObserver observer = new TracingObserver(tracer);

        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        observer.onRecord(ctx, 100L);
        observer.onRecord(ctx, 200L);

        List<SpanData> spans = exporter.getFinishedSpanItems();
        long recordSpans = spans.stream().filter(s -> s.getName().equals(TracingObserver.TRACING_SPAN_NAME)).count();
        assertEquals(2L, recordSpans);
        SpanData first = spans.stream()
                .filter(s -> s.getName().equals(TracingObserver.TRACING_SPAN_NAME))
                .findFirst()
                .orElseThrow();
        assertEquals("put", first.getAttributes().get(io.opentelemetry.api.common.AttributeKey.stringKey("subms.stage")));
        assertEquals("wl", first.getAttributes().get(io.opentelemetry.api.common.AttributeKey.stringKey("subms.workload")));
    }

    @Test
    void recordSpanInheritsActiveParent() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        SdkTracerProvider provider = buildProvider(exporter);
        Tracer tracer = provider.get("subms-otel-tracing-test");

        Span parent = tracer.spanBuilder("http-request").startSpan();
        try (Scope ignored = Context.current().with(parent).makeCurrent()) {
            TracingObserver observer = new TracingObserver(tracer);
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            observer.onRecord(ctx, 500L);
        }
        parent.end();

        List<SpanData> spans = exporter.getFinishedSpanItems();
        SpanData parentSpan = spans.stream()
                .filter(s -> s.getName().equals("http-request"))
                .findFirst()
                .orElseThrow();
        Optional<SpanData> child = spans.stream()
                .filter(s -> s.getName().equals(TracingObserver.TRACING_SPAN_NAME))
                .findFirst();
        assertTrue(child.isPresent(), "record span should be present");
        assertEquals(parentSpan.getSpanContext().getSpanId(), child.get().getParentSpanId(),
                "record span should inherit the active span as its parent");
        assertEquals(parentSpan.getSpanContext().getTraceId(), child.get().getSpanContext().getTraceId(),
                "record span should share the parent's trace id");
    }

    @Test
    void rootSpanWhenNoParentIsActive() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        SdkTracerProvider provider = buildProvider(exporter);
        Tracer tracer = provider.get("root-test");
        TracingObserver observer = new TracingObserver(tracer);
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        observer.onRecord(ctx, 250L);
        List<SpanData> spans = exporter.getFinishedSpanItems();
        assertEquals(1, spans.size());
        // No parent active means parent span id is the invalid sentinel.
        assertEquals("0000000000000000", spans.get(0).getParentSpanId());
    }

    @Test
    void onSummarizeIsNoOp() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        SdkTracerProvider provider = buildProvider(exporter);
        Tracer tracer = provider.get("summarize-test");
        TracingObserver observer = new TracingObserver(tracer);
        // Synthetic summary: building a real one requires a harness; just confirm the call is safe.
        observer.onSummarize(null);
        assertEquals(0, exporter.getFinishedSpanItems().size());
    }

    @Test
    void constructorRejectsNullTracer() {
        assertThrows(NullPointerException.class, () -> new TracingObserver(null));
    }

    @Test
    void spansCarryStageKindAttribute() {
        InMemorySpanExporter exporter = InMemorySpanExporter.create();
        SdkTracerProvider provider = buildProvider(exporter);
        Tracer tracer = provider.get("kind-test");
        TracingObserver observer = new TracingObserver(tracer);
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "compact", SubMsStageKind.BATCH_OP);
        observer.onRecord(ctx, 1_000_000L);
        SpanData s = exporter.getFinishedSpanItems().get(0);
        assertNotNull(s);
        assertEquals("batch_op",
                s.getAttributes().get(io.opentelemetry.api.common.AttributeKey.stringKey("subms.stage.kind")));
    }
}
