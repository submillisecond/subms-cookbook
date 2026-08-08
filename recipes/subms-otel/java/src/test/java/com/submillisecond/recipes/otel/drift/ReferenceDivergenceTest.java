package com.submillisecond.recipes.otel.drift;

import com.submillisecond.recipes.otel.OtelObserver;
import com.submillisecond.recipes.otel.OtelObserverAsync;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.AttributeKey;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.LongPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import com.submillisecond.recipes.otel.testing.InMemoryMetricExporter;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

class ReferenceDivergenceTest {

    private static SdkMeterProvider buildProvider(InMemoryMetricExporter exporter) {
        return SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
    }

    private static long counterSum(List<MetricData> metrics, String name) {
        return metrics.stream()
                .filter(m -> m.getName().equals(name))
                .findFirst()
                .map(m -> m.getLongSumData().getPoints().stream().mapToLong(LongPointData::getValue).sum())
                .orElse(0L);
    }

    @Test
    void attributesCarryExpectedKeys() {
        Attributes a = ReferenceDivergenceRecorder.divergenceAttributes(
                "contains", SubMsStageKind.HOT_PATH, "set_membership", "true", "false");
        assertEquals("contains", a.get(AttributeKey.stringKey("subms.stage")));
        assertEquals("hot_path", a.get(AttributeKey.stringKey("subms.stage.kind")));
        assertEquals("set_membership", a.get(AttributeKey.stringKey(ReferenceDivergenceRecorder.REFERENCE_KIND_ATTR)));
        assertEquals("true", a.get(AttributeKey.stringKey(ReferenceDivergenceRecorder.REFERENCE_EXPECTED_ATTR)));
        assertEquals("false", a.get(AttributeKey.stringKey(ReferenceDivergenceRecorder.REFERENCE_OBSERVED_ATTR)));
    }

    @Test
    void recorderFiresCounter() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        ReferenceDivergenceRecorder rec = new ReferenceDivergenceRecorder(provider.get("drift-test"));
        rec.record("contains", SubMsStageKind.HOT_PATH, "set_membership", "true", "false");
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        long total = counterSum(exporter.getFinishedMetricItems(),
                ReferenceDivergenceRecorder.REFERENCE_DIVERGENCE_COUNTER_NAME);
        assertEquals(1L, total);
    }

    @Test
    void recorderReusesCounterAcrossCalls() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        ReferenceDivergenceRecorder rec = new ReferenceDivergenceRecorder(provider.get("drift-test"));
        for (int i = 0; i < 5; i++) {
            rec.record("estimate", SubMsStageKind.HOT_PATH, "count_estimate", "1000", "997");
        }
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        long total = counterSum(exporter.getFinishedMetricItems(),
                ReferenceDivergenceRecorder.REFERENCE_DIVERGENCE_COUNTER_NAME);
        assertEquals(5L, total);
    }

    @Test
    void syncObserverExposesPublicMethod() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        OtelObserver observer = new OtelObserver(provider.get("drift-test"));
        observer.recordReferenceDivergence(
                "lookup", SubMsStageKind.HOT_PATH, "set_membership", "true", "false");
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        long total = counterSum(exporter.getFinishedMetricItems(),
                ReferenceDivergenceRecorder.REFERENCE_DIVERGENCE_COUNTER_NAME);
        assertEquals(1L, total);
    }

    @Test
    void asyncObserverExposesPublicMethod() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        try (OtelObserverAsync observer = new OtelObserverAsync(provider.get("drift-test"), 32, 10L, false)) {
            observer.recordReferenceDivergence(
                    "lookup", SubMsStageKind.HOT_PATH, "set_membership", "true", "false");
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            long total = counterSum(exporter.getFinishedMetricItems(),
                    ReferenceDivergenceRecorder.REFERENCE_DIVERGENCE_COUNTER_NAME);
            assertEquals(1L, total);
        }
    }

    @Test
    void recorderRejectsNullMeter() {
        assertThrows(NullPointerException.class, () -> new ReferenceDivergenceRecorder(null));
    }

    @Test
    void recordRejectsNullArgs() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        ReferenceDivergenceRecorder rec = new ReferenceDivergenceRecorder(provider.get("drift-test"));
        assertThrows(NullPointerException.class,
                () -> rec.record(null, SubMsStageKind.HOT_PATH, "k", "e", "o"));
        assertThrows(NullPointerException.class,
                () -> rec.record("s", null, "k", "e", "o"));
        assertThrows(NullPointerException.class,
                () -> rec.record("s", SubMsStageKind.HOT_PATH, null, "e", "o"));
        assertNotNull(rec);
    }
}
