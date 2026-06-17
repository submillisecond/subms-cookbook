package com.submillisecond.otel;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsStageSummary;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.LongPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import com.submillisecond.otel.testing.InMemoryMetricExporter;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OtelObserverAsyncTest {

    @Test
    void drainsAllRecordsAfterForceDrain() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        // No background drainer so the test owns when drain fires; avoids a race between drainNow()
        // and the drain thread's poll-then-emit window.
        OtelObserverAsync async = new OtelObserverAsync(provider.get("subms-otel-test"), 200_000, 10_000L, false);
        try {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            int n = 100_000;
            for (int i = 0; i < n; i++) async.onRecord(ctx, 50 + i);

            async.drainNow();
            provider.forceFlush().join(10, TimeUnit.SECONDS);

            MetricData latency = exporter.getFinishedMetricItems().stream()
                    .filter(m -> m.getName().equals("subms.latency"))
                    .findFirst()
                    .orElseThrow();
            assertEquals("s", latency.getUnit());
            long total = 0L;
            for (HistogramPointData p : latency.getHistogramData().getPoints()) {
                assertEquals("put", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
                total += p.getCount();
            }
            assertEquals(n, total);
            assertEquals(0L, async.droppedCount(), "no drops below capacity");
        } finally {
            async.close();
        }
    }

    @Test
    void backPressureDropsExcessSamplesAndCounts() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        // No drainer thread so the test owns when drain fires; producer can only fill 64 slots.
        OtelObserverAsync async = new OtelObserverAsync(provider.get("subms-otel-test"), 64, 10_000L, false);
        try {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            int produced = 10_000;
            for (int i = 0; i < produced; i++) async.onRecord(ctx, 50 + i);

            // Some MUST have been dropped given a 64-deep queue and no draining yet.
            assertTrue(async.droppedCount() > 0L,
                    "expected drops with tiny queue, got " + async.droppedCount());
            assertEquals(produced - async.droppedCount(), async.queueDepth());

            async.drainNow();
            provider.forceFlush().join(5, TimeUnit.SECONDS);

            MetricData latency = findMetric(exporter.getFinishedMetricItems(), "subms.latency");
            long total = 0L;
            for (HistogramPointData p : latency.getHistogramData().getPoints()) total += p.getCount();
            assertEquals(produced - async.droppedCount(), total);

            MetricData dropped = findMetric(exporter.getFinishedMetricItems(), OtelObserverAsync.DROPPED_COUNTER_NAME);
            long sumDropped = 0L;
            for (LongPointData p : dropped.getLongSumData().getPoints()) sumDropped += p.getValue();
            assertEquals(async.droppedCount(), sumDropped);
        } finally {
            async.close();
        }
    }

    @Test
    void onSummariseDrainsThenSwapsAttributeSet() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("subms-otel-test"), 1024, 10_000L)) {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
            for (int i = 0; i < 10; i++) async.onRecord(ctx, 100 + i);

            // First summary: also triggers exportSummary with full attrs.
            SubMsBenchSummary summary = SubMsOtelTest.buildSummary(false);
            async.onSummarize(summary);
            assertEquals(0, async.queueDepth(), "drainNow inside onSummarize cleared queue");

            // Post-summary records should now carry the full attribute set.
            for (int i = 0; i < 5; i++) async.onRecord(ctx, 200 + i);
            async.drainNow();
            provider.forceFlush().join(5, TimeUnit.SECONDS);

            MetricData latency = findMetric(exporter.getFinishedMetricItems(), "subms.latency");
            boolean sawFullAttrs = latency.getHistogramData().getPoints().stream()
                    .anyMatch(p -> "ci-runner".equals(p.getAttributes().get(SubMsOtelAttributeKeys.KEY_HOST)));
            assertTrue(sawFullAttrs, "post-summary records should carry summary attribute set");
        }
    }

    @Test
    void closeIsIdempotent() {
        SdkMeterProvider provider = SdkMeterProvider.builder().build();
        OtelObserverAsync async = new OtelObserverAsync(provider.get("t"));
        async.close();
        async.close(); // second call must be a no-op
    }

    @Test
    void constructorValidation() {
        SdkMeterProvider provider = SdkMeterProvider.builder().build();
        assertThrows(NullPointerException.class, () -> new OtelObserverAsync(null));
        assertThrows(IllegalArgumentException.class,
                () -> new OtelObserverAsync(provider.get("t"), 0, 100L));
        assertThrows(IllegalArgumentException.class,
                () -> new OtelObserverAsync(provider.get("t"), 64, 0L));
    }

    @Test
    void drainThreadDrainsContinuouslyInBackground() throws Exception {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("subms-otel-test"), 4096, 20L)) {
            SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "tick", SubMsStageKind.HOT_PATH);
            for (int i = 0; i < 1000; i++) async.onRecord(ctx, 100L);

            long deadline = System.currentTimeMillis() + 5000;
            while (async.queueDepth() > 0 && System.currentTimeMillis() < deadline) {
                Thread.sleep(20);
            }
            assertEquals(0, async.queueDepth(), "background drainer should empty queue");
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            MetricData latency = findMetric(exporter.getFinishedMetricItems(), "subms.latency");
            long total = 0L;
            for (HistogramPointData p : latency.getHistogramData().getPoints()) {
                assertEquals("tick", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
                total += p.getCount();
            }
            assertEquals(1000L, total);
        }
    }

    @Test
    void onSummariseAttributesLandWithoutPriorRecords() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        try (OtelObserverAsync async = new OtelObserverAsync(provider.get("subms-otel-test"), 64, 10_000L)) {
            SubMsBenchSummary summary = new SubMsBenchSummary(
                    "wl", "java", "ts",
                    Map.of(), Map.of(),
                    List.of(new SubMsStageSummary("put", 1, 1, 1, 1, 1, 1, 0, Optional.empty())));
            async.onSummarize(summary);
            provider.forceFlush().join(5, TimeUnit.SECONDS);
            MetricData latency = findMetric(exporter.getFinishedMetricItems(), "subms.latency");
            assertNotNull(latency);
            assertEquals("s", latency.getUnit());
        }
    }

    static MetricData findMetric(List<MetricData> metrics, String name) {
        return metrics.stream()
                .filter(m -> m.getName().equals(name))
                .findFirst()
                .orElseThrow(() -> new AssertionError("missing metric: " + name + " in " + metrics));
    }
}
