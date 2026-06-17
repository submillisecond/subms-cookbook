package com.submillisecond.otel;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import com.submillisecond.otel.testing.InMemoryMetricExporter;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OtelObserverTest {

    @Test
    void recordsAndSummarisesThroughHarness() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-otel-test");
        OtelObserver observer = new OtelObserver(meter);

        SubMsPerfHarness h = new SubMsPerfHarness("wl", "java");
        h.input("entries", "100");
        h.meta("host", "kr-laptop");
        h.withObserver(observer);

        SubMsPerfHarness.Stage stage = h.stage("put", 100).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < 100; i++) stage.record(50 + i);

        SubMsBenchSummary summary = SubMsBench.summarize(h);
        assertNotNull(summary);
        provider.forceFlush().join(5, TimeUnit.SECONDS);

        List<MetricData> metrics = exporter.getFinishedMetricItems();
        MetricData latency = metrics.stream()
                .filter(m -> m.getName().equals("subms.latency"))
                .findFirst()
                .orElseThrow();
        assertEquals("s", latency.getUnit());

        // 100 hot-path records (lean attrs) + 6 percentiles + 100 downsampled samples = 206
        // across however many attribute-set points (lean vs full).
        long total = 0L;
        for (HistogramPointData p : latency.getHistogramData().getPoints()) {
            // Every point under the single instrument must carry the put stage.
            assertEquals("put", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
            total += p.getCount();
        }
        assertEquals(206L, total);

        // host attribute came in via the summary attribute set
        boolean sawHostAttribute = latency.getHistogramData().getPoints().stream()
                .anyMatch(p -> "kr-laptop".equals(p.getAttributes().get(SubMsOtelAttributeKeys.KEY_HOST)));
        assertTrue(sawHostAttribute);
    }

    @Test
    void onRecordCarriesLeanAttributesOnly() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        OtelObserver observer = new OtelObserver(provider.get("subms-otel-test"));

        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        observer.onRecord(ctx, 100L);
        observer.onRecord(ctx, 200L);
        observer.onRecord(ctx, 300L);
        provider.forceFlush().join(5, TimeUnit.SECONDS);

        MetricData latency = exporter.getFinishedMetricItems().stream()
                .filter(m -> m.getName().equals("subms.latency"))
                .findFirst()
                .orElseThrow();
        assertEquals("s", latency.getUnit());
        HistogramPointData point = latency.getHistogramData().getPoints().iterator().next();

        // No recipe.slug / host / hardware_tier on the lean ctx attribute set.
        assertEquals("wl", point.getAttributes().get(SubMsOtelAttributeKeys.KEY_WORKLOAD));
        assertEquals("java", point.getAttributes().get(SubMsOtelAttributeKeys.KEY_LANG));
        assertEquals("put", point.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
        assertEquals(SubMsStageKind.HOT_PATH.asString(),
                point.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE_KIND));
        assertNull(point.getAttributes().get(SubMsOtelAttributeKeys.KEY_HOST));
        assertEquals(3L, point.getCount());
        // Values land in seconds (ns / 1e9). 100+200+300 = 600 ns -> 6e-7 s.
        assertEquals(6e-7, point.getSum(), 1e-12);
    }

    @Test
    void instrumentIsCachedAcrossCalls() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        OtelObserver observer = new OtelObserver(provider.get("subms-otel-test"));

        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        // Two calls touching the same stage; second one must reuse the cached histogram.
        observer.onRecord(ctx, 100L);
        observer.onRecord(ctx, 200L);
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        long histograms = exporter.getFinishedMetricItems().stream()
                .filter(m -> m.getName().equals("subms.latency"))
                .count();
        assertEquals(1, histograms);
    }

    @Test
    void constructorRejectsNullMeter() {
        assertThrows(NullPointerException.class, () -> new OtelObserver(null));
    }
}
