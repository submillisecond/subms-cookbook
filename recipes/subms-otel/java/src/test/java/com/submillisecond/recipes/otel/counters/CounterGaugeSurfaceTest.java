package com.submillisecond.recipes.otel.counters;

import com.submillisecond.recipes.otel.OtelObserver;
import com.submillisecond.recipes.otel.OtelObserverAsync;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
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
import static org.junit.jupiter.api.Assertions.assertTrue;

class CounterGaugeSurfaceTest {

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

    private static long gaugeMax(List<MetricData> metrics, String name) {
        return metrics.stream()
                .filter(m -> m.getName().equals(name))
                .findFirst()
                .map(m -> m.getLongGaugeData().getPoints().stream().mapToLong(LongPointData::getValue).max().orElse(-1L))
                .orElse(-1L);
    }

    @Test
    void syncObserverIncrementsOpsTotalPerRecord() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        OtelObserver observer = new OtelObserver(provider.get("ops-test"));
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        for (int i = 0; i < 50; i++) observer.onRecord(ctx, 50 + i);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        long total = counterSum(exporter.getFinishedMetricItems(), OtelObserver.OPS_TOTAL_COUNTER_NAME);
        assertEquals(50L, total);
    }

    @Test
    void asyncObserverIncrementsOpsTotalPerRecord() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("ops-test"), 4096, 10L, false)) {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            for (int i = 0; i < 100; i++) async.onRecord(ctx, 50 + i);
            async.drainNow();
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            long total = counterSum(exporter.getFinishedMetricItems(), OtelObserverAsync.OPS_TOTAL_COUNTER_NAME);
            assertEquals(100L, total);
        }
    }

    @Test
    void inFlightGaugeObservesQueueDepth() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("in-flight-test"), 1024, 10_000L, false)) {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            for (int i = 0; i < 200; i++) async.onRecord(ctx, 50 + i);
            // Flush BEFORE drain so the gauge observes the non-empty queue.
            int depthBefore = async.queueDepth();
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            long gauge = gaugeMax(exporter.getFinishedMetricItems(), OtelObserverAsync.IN_FLIGHT_GAUGE_NAME);
            assertTrue(gauge >= 1L, "expected non-zero queue depth observation, depthBefore=" + depthBefore + " gauge=" + gauge);
            async.drainNow();
        }
    }

    @Test
    void droppedTotalSurvivesRename() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = buildProvider(exporter);
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("drop-test"), 8, 10_000L, false)) {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            for (int i = 0; i < 100; i++) async.onRecord(ctx, 50 + i);
            assertTrue(async.droppedCount() > 0L);
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            long dropped = counterSum(exporter.getFinishedMetricItems(), OtelObserverAsync.DROPPED_TOTAL_COUNTER_NAME);
            assertEquals(async.droppedCount(), dropped);
        }
    }
}
