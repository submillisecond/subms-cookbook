package com.submillisecond.otel.exporters;

import io.opentelemetry.exporter.otlp.http.metrics.OtlpHttpMetricExporter;
import io.opentelemetry.exporter.otlp.http.trace.OtlpHttpSpanExporter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.resources.Resource;
import io.opentelemetry.sdk.trace.SdkTracerProvider;
import io.opentelemetry.sdk.trace.export.BatchSpanProcessor;

import java.time.Duration;

/**
 * Hands back wired {@link SdkMeterProvider} + {@link SdkTracerProvider} backed by OTLP. The protocol
 * argument picks HTTP/proto vs gRPC; this helper only ships the HTTP path (the wider grpc bundle pulls
 * extra deps).
 */
public final class ExporterOtlpHelper {

    public enum Protocol {
        HTTP_PROTOBUF,
        GRPC;

        public static Protocol fromEnv(String value) {
            if (value == null) return HTTP_PROTOBUF;
            String trimmed = value.trim();
            if (trimmed.equalsIgnoreCase("grpc")) return GRPC;
            return HTTP_PROTOBUF;
        }
    }

    private ExporterOtlpHelper() {}

    public static Wired build(String endpoint, Protocol protocol, Resource resource) {
        var metricBuilder = OtlpHttpMetricExporter.builder();
        if (endpoint != null && !endpoint.isEmpty()) {
            metricBuilder.setEndpoint(endpoint);
        }
        SdkMeterProvider meterProvider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(metricBuilder.build())
                        .setInterval(Duration.ofSeconds(10))
                        .build())
                .setResource(resource)
                .build();

        var spanBuilder = OtlpHttpSpanExporter.builder();
        if (endpoint != null && !endpoint.isEmpty()) {
            spanBuilder.setEndpoint(endpoint);
        }
        SdkTracerProvider tracerProvider = SdkTracerProvider.builder()
                .addSpanProcessor(BatchSpanProcessor.builder(spanBuilder.build()).build())
                .setResource(resource)
                .build();
        return new Wired(meterProvider, tracerProvider);
    }

    public record Wired(SdkMeterProvider meterProvider, SdkTracerProvider tracerProvider) {}
}
