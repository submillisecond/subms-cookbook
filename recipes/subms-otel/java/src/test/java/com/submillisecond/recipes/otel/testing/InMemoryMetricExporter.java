package com.submillisecond.recipes.otel.testing;

import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.metrics.InstrumentType;
import io.opentelemetry.sdk.metrics.data.AggregationTemporality;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.MetricExporter;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;

public final class InMemoryMetricExporter implements MetricExporter {

    private final List<MetricData> finished = Collections.synchronizedList(new ArrayList<>());

    private InMemoryMetricExporter() {}

    public static InMemoryMetricExporter create() {
        return new InMemoryMetricExporter();
    }

    public List<MetricData> getFinishedMetricItems() {
        synchronized (finished) {
            return new ArrayList<>(finished);
        }
    }

    public void reset() {
        finished.clear();
    }

    @Override
    public AggregationTemporality getAggregationTemporality(InstrumentType instrumentType) {
        return AggregationTemporality.CUMULATIVE;
    }

    @Override
    public CompletableResultCode export(Collection<MetricData> metrics) {
        finished.addAll(metrics);
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
