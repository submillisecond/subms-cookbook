package com.submillisecond.recipes.otel.tracing;

import com.submillisecond.recipes.otel.SubMsOtel;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.trace.Span;
import io.opentelemetry.api.trace.SpanBuilder;
import io.opentelemetry.api.trace.SpanContext;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.context.Context;

import java.time.Instant;
import java.util.Objects;

/**
 * {@link SubMsObserver} that emits a short OTEL span per {@code onRecord} call.
 *
 * <p>Span start is reconstructed as {@code now - ns} so the duration matches the harness's
 * measured nanoseconds. If a calling thread already has an active span (e.g. a Spring HTTP
 * filter started a trace), the {@link Context#current()} parent is inherited; otherwise the
 * span is rooted.
 *
 * <p>{@link #onSummarize(SubMsBenchSummary)} is intentionally a no-op - the post-bench
 * narrative is already covered by
 * {@link SubMsOtel#exportTimer(com.submillisecond.perf.SubMsTimer, Tracer)} +
 * {@link SubMsOtel#exportSummary(SubMsBenchSummary, io.opentelemetry.api.metrics.Meter)}.
 */
public final class TracingObserver implements SubMsObserver {

    /** Span name used for every emitted record. Stage identity is on attrs. */
    public static final String TRACING_SPAN_NAME = "subms.stage.record";

    private final Tracer tracer;

    public TracingObserver(Tracer tracer) {
        this.tracer = Objects.requireNonNull(tracer, "tracer");
    }

    @Override
    public void onRecord(SubMsObservationCtx ctx, long ns) {
        Instant end = Instant.now();
        Instant start = end.minusNanos(ns);
        Attributes attrs = SubMsOtel.ctxAttributes(
                ctx.workload(), ctx.lang(), ctx.stage(), ctx.stageKind());
        Context parentCtx = Context.current();
        SpanBuilder b = tracer.spanBuilder(TRACING_SPAN_NAME)
                .setParent(parentCtx)
                .setStartTimestamp(start)
                .setAllAttributes(attrs);
        Span span = b.startSpan();
        SpanContext sc = span.getSpanContext();
        if (sc != null && sc.isValid()) {
            // touch to prevent the unused-var warning across surfaces that don't read it
        }
        span.end(end);
    }

    @Override
    public void onSummarize(SubMsBenchSummary summary) {
        // intentional no-op
    }
}
