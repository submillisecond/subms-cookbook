package com.submillisecond.otel.testing;

import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.trace.data.SpanData;
import io.opentelemetry.sdk.trace.export.SpanExporter;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;

public final class InMemorySpanExporter implements SpanExporter {

    private final List<SpanData> finished = Collections.synchronizedList(new ArrayList<>());

    private InMemorySpanExporter() {}

    public static InMemorySpanExporter create() {
        return new InMemorySpanExporter();
    }

    public List<SpanData> getFinishedSpanItems() {
        synchronized (finished) {
            return new ArrayList<>(finished);
        }
    }

    public void reset() {
        finished.clear();
    }

    @Override
    public CompletableResultCode export(Collection<SpanData> spans) {
        finished.addAll(spans);
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode flush() {
        return CompletableResultCode.ofSuccess();
    }

    @Override
    public CompletableResultCode shutdown() {
        finished.clear();
        return CompletableResultCode.ofSuccess();
    }
}
