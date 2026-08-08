package com.submillisecond.recipes.otel.exporters;

import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SpanExporter;

import java.io.PrintStream;
import java.util.Collection;

/**
 * Hand-rolled {@link SpanExporter} that prints every flushed span as a JSON-ish
 * line on a {@link PrintStream}. Pairs with {@link StdoutMetricExporter} for
 * the bridge's local-debugging fallback.
 */
public final class StdoutSpanExporter implements SpanExporter {

    private final PrintStream out;

    public StdoutSpanExporter(PrintStream out) {
        this.out = out;
    }

    public static StdoutSpanExporter create() {
        return new StdoutSpanExporter(System.out);
    }

    @Override
    public CompletableResultCode export(Collection<SpanData> spans) {
        for (SpanData s : spans) {
            out.printf("[otel] span:   name=%s  trace=%s  span=%s  attrs=%s%n",
                    s.getName(),
                    s.getSpanContext().getTraceId(),
                    s.getSpanContext().getSpanId(),
                    s.getAttributes());
        }
        out.flush();
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode flush() {
        out.flush();
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode shutdown() {
        return flush();
    }
}
