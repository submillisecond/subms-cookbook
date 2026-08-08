package com.submillisecond.recipes.otel.exporters;

import com.submillisecond.recipes.otel.resource.SubMsOtelResource;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.resources.Resource;
import org.junit.jupiter.api.Test;

import java.net.ServerSocket;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotNull;

class ExporterPrometheusHelperTest {

    @Test
    void buildsAndAcceptsRecord() throws Exception {
        int port;
        try (ServerSocket s = new ServerSocket(0)) {
            port = s.getLocalPort();
        }
        Resource resource = SubMsOtelResource.detect();
        ExporterPrometheusHelper.Wired wired = ExporterPrometheusHelper.build(resource, port);
        assertNotNull(wired.meterProvider());
        assertNotNull(wired.reader());
        Meter meter = wired.meterProvider().meterBuilder("smoke").build();
        DoubleHistogram h = meter.histogramBuilder("smoke.latency").build();
        assertDoesNotThrow(() -> h.record(0.0001));
        wired.meterProvider().close();
    }
}
