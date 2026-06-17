package com.submillisecond.otel.exporters;

import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.resources.Resource;

import java.time.Duration;

/**
 * Hands back wired {@link SdkMeterProvider} backed by the in-tree
 * {@link PrometheusHttpServer} (HTTP scrape exporter). Prometheus is
 * metrics-only - no tracer is returned; autoconfig pairs this with a
 * no-op tracer provider.
 *
 * <p>Defaults: bound to {@code 0.0.0.0:9464}.
 */
public final class ExporterPrometheusHelper {

    public static final int DEFAULT_PORT = 9464;

    private ExporterPrometheusHelper() {}

    public static Wired build(Resource resource) {
        return build(resource, DEFAULT_PORT);
    }

    public static Wired build(Resource resource, int port) {
        PrometheusHttpServer reader = PrometheusHttpServer.builder().setPort(port).build();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(reader)
                        .setInterval(Duration.ofSeconds(15))
                        .build())
                .setResource(resource)
                .build();
        return new Wired(provider, reader);
    }

    public record Wired(SdkMeterProvider meterProvider, PrometheusHttpServer reader) {}
}
