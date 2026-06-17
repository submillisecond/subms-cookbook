package com.submillisecond.primers.otel;

import io.opentelemetry.sdk.common.CompletableResultCode;
import io.opentelemetry.sdk.metrics.InstrumentType;
import io.opentelemetry.sdk.metrics.data.AggregationTemporality;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.data.MetricDataType;
import io.opentelemetry.sdk.metrics.data.PointData;
import io.opentelemetry.sdk.metrics.export.MetricExporter;

import java.io.PrintStream;
import java.util.Collection;

/**
 * Self-contained {@link MetricExporter} that prints every flushed metric to a
 * {@link PrintStream}. Replaces the heavier {@code opentelemetry-exporter-logging}
 * artefact so the primer stays dependency-light and runs offline.
 *
 * <p>One block per metric: name, unit, then a per-point attribute set + count
 * + sum. Adequate for the primer's "look, OTEL data came out" demonstration;
 * not a substitute for OTLP in production.
 */
public final class StdoutMetricExporter implements MetricExporter {

    private final PrintStream out;

    public StdoutMetricExporter(PrintStream out) {
        this.out = out;
    }

    /** Convenience: build one that writes to {@link System#out}. */
    public static StdoutMetricExporter create() {
        return new StdoutMetricExporter(System.out);
    }

    @Override
    public AggregationTemporality getAggregationTemporality(InstrumentType instrumentType) {
        return AggregationTemporality.CUMULATIVE;
    }

    @Override
    public CompletableResultCode export(Collection<MetricData> metrics) {
        for (MetricData m : metrics) {
            out.println();
            out.printf("[otel] metric: %s  unit=%s  type=%s%n",
                    m.getName(), m.getUnit(), m.getType());
            if (m.getType() == MetricDataType.HISTOGRAM) {
                for (HistogramPointData p : m.getHistogramData().getPoints()) {
                    out.printf("        attrs=%s  count=%d  sum=%.9fs%n",
                            p.getAttributes(), p.getCount(), p.getSum());
                }
            } else {
                for (PointData p : m.getData().getPoints()) {
                    out.printf("        attrs=%s  point=%s%n", p.getAttributes(), p);
                }
            }
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
