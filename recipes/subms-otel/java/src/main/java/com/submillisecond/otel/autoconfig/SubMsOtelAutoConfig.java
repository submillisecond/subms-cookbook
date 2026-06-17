package com.submillisecond.otel.autoconfig;

import com.submillisecond.perf.SubMsObserver;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.trace.SdkTracerProvider;

/**
 * Configured providers + handles returned from {@link SubMsOtelBootstrap#autoConfigure()}.
 *
 * <p>The caller registers {@link #observer} on the harness via {@code setObserver} and is expected to call
 * {@link SdkMeterProvider#close()} / {@link SdkTracerProvider#close()} at program exit.
 */
public record SubMsOtelAutoConfig(
        Meter meter,
        Tracer tracer,
        SubMsObserver observer,
        SdkMeterProvider meterProvider,
        SdkTracerProvider tracerProvider) {
}
