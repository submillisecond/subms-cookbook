package com.submillisecond.recipes.otel.exporters;

import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.resources.Resource;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.export.SimpleSpanProcessor;

import java.time.Duration;

/**
 * Hands back wired {@link SdkMeterProvider} + {@link SdkTracerProvider} backed by the in-tree
 * {@link StdoutMetricExporter} + {@link StdoutSpanExporter}. Useful as the autoconfig fallback when no
 * {@code OTEL_EXPORTER_OTLP_ENDPOINT} is set but a developer still wants to see emissions during local
 * debugging.
 */
public final class ExporterStdoutHelper {

    private ExporterStdoutHelper() {}

    public static Wired build(Resource resource) {
        SdkMeterProvider meterProvider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(StdoutMetricExporter.create())
                        .setInterval(Duration.ofSeconds(60))
                        .build())
                .setResource(resource)
                .build();

        SdkTracerProvider tracerProvider = SdkTracerProvider.builder()
                .addSpanProcessor(SimpleSpanProcessor.create(StdoutSpanExporter.create()))
                .setResource(resource)
                .build();
        return new Wired(meterProvider, tracerProvider);
    }

    public record Wired(SdkMeterProvider meterProvider, SdkTracerProvider tracerProvider) {}
}
