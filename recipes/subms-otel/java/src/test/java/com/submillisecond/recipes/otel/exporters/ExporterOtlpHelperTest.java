package com.submillisecond.recipes.otel.exporters;

import com.submillisecond.recipes.otel.resource.SubMsOtelResource;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.resources.Resource;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

class ExporterOtlpHelperTest {

    @Test
    void protocolFromEnv() {
        assertEquals(ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, ExporterOtlpHelper.Protocol.fromEnv(null));
        assertEquals(ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, ExporterOtlpHelper.Protocol.fromEnv("http/protobuf"));
        assertEquals(ExporterOtlpHelper.Protocol.GRPC, ExporterOtlpHelper.Protocol.fromEnv("grpc"));
        assertEquals(ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, ExporterOtlpHelper.Protocol.fromEnv("unknown"));
    }

    @Test
    void buildsAndAcceptsRecord() {
        Resource resource = SubMsOtelResource.detect();
        ExporterOtlpHelper.Wired wired =
                ExporterOtlpHelper.build(null, ExporterOtlpHelper.Protocol.HTTP_PROTOBUF, resource);
        assertNotNull(wired.meterProvider());
        assertNotNull(wired.tracerProvider());
        Meter meter = wired.meterProvider().meterBuilder("smoke").build();
        DoubleHistogram h = meter.histogramBuilder("smoke.latency").build();
        assertDoesNotThrow(() -> h.record(0.0001));
        wired.meterProvider().close();
        wired.tracerProvider().close();
    }
}
