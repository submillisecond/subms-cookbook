package com.submillisecond.recipes.otel.exporters;

import com.submillisecond.recipes.otel.resource.SubMsOtelResource;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Span;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.sdk.resources.Resource;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotNull;

class ExporterStdoutHelperTest {

    @Test
    void buildsWiredProvidersAndAcceptsEmissions() {
        Resource resource = SubMsOtelResource.detect();
        ExporterStdoutHelper.Wired wired = ExporterStdoutHelper.build(resource);
        assertNotNull(wired.meterProvider());
        assertNotNull(wired.tracerProvider());

        Meter meter = wired.meterProvider().meterBuilder("smoke").build();
        DoubleHistogram h = meter.histogramBuilder("smoke.latency").build();
        assertDoesNotThrow(() -> h.record(0.0001));

        Tracer tracer = wired.tracerProvider().tracerBuilder("smoke").build();
        Span span = tracer.spanBuilder("smoke.span").startSpan();
        assertDoesNotThrow((org.junit.jupiter.api.function.Executable) span::end);

        wired.meterProvider().close();
        wired.tracerProvider().close();
    }
}
