package com.submillisecond.primers.otel;

import com.submillisecond.otel.OtelObserver;
import com.submillisecond.otel.SubMsOtelAttributeKeys;
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsPerfHarness;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.HistogramPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import io.opentelemetry.sdk.testing.exporter.InMemoryMetricExporter;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.stream.Collectors;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * End-to-end: register an {@link OtelObserver}, drive the {@link Workload}
 * through the harness, force-flush the SDK, then inspect the captured
 * {@code subms.latency} histogram via the SDK's
 * {@link InMemoryMetricExporter}. The same metric / attribute set every
 * cookbook recipe will emit once it wires the observer in.
 */
final class OtelObserverIntegrationTest {

    private static final int ENTRIES = 200;

    private InMemoryMetricExporter exporter;
    private SdkMeterProvider provider;
    private Meter meter;

    @BeforeEach
    void buildSdk() {
        exporter = InMemoryMetricExporter.create();
        provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        meter = provider.get("subms-primer-otel/itest");
    }

    @AfterEach
    void tearDown() {
        if (provider != null) provider.close();
    }

    private MetricData runAndCaptureLatency() {
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java")
                .withObserver(new OtelObserver(meter));
        Workload.runWorkload(h, ENTRIES);
        SubMsBenchSummary summary = SubMsBench.summarize(h);
        assertNotNull(summary);
        provider.forceFlush().join(5, TimeUnit.SECONDS);

        return exporter.getFinishedMetricItems().stream()
                .filter(m -> m.getName().equals("subms.latency"))
                .findFirst()
                .orElseThrow();
    }

    @Test
    @DisplayName("workload emits a metric named subms.latency with unit s")
    void emitsCanonicalLatencyMetric() {
        MetricData latency = runAndCaptureLatency();
        assertEquals("subms.latency", latency.getName());
        assertEquals("s", latency.getUnit());
    }

    @Test
    @DisplayName("every captured histogram point carries the standard subms.* attribute set")
    void everyPointCarriesStandardAttributes() {
        MetricData latency = runAndCaptureLatency();
        for (HistogramPointData p : latency.getHistogramData().getPoints()) {
            assertEquals("subms-primer-otel", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_WORKLOAD));
            assertEquals("java", p.getAttributes().get(SubMsOtelAttributeKeys.KEY_LANG));
            assertNotNull(p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE),
                    "every point must carry subms.stage");
        }
    }

    @Test
    @DisplayName("all three stages show up: put, get_hit, get_miss")
    void allThreeStagesPresent() {
        MetricData latency = runAndCaptureLatency();
        Set<String> stages = latency.getHistogramData().getPoints().stream()
                .map(p -> p.getAttributes().get(SubMsOtelAttributeKeys.KEY_STAGE))
                .collect(Collectors.toSet());
        assertTrue(stages.contains("put"));
        assertTrue(stages.contains("get_hit"));
        assertTrue(stages.contains("get_miss"));
    }

    @Test
    @DisplayName("at least one point carries the recipe.slug / recipe.category set from the summary pass")
    void summaryPassCarriesRecipeIdentity() {
        MetricData latency = runAndCaptureLatency();
        boolean sawSlug = latency.getHistogramData().getPoints().stream()
                .anyMatch(p -> "subms-primer-otel".equals(
                        p.getAttributes().get(SubMsOtelAttributeKeys.KEY_RECIPE_SLUG)));
        boolean sawCategory = latency.getHistogramData().getPoints().stream()
                .anyMatch(p -> "tooling".equals(
                        p.getAttributes().get(SubMsOtelAttributeKeys.KEY_RECIPE_CATEGORY)));
        assertTrue(sawSlug, "summary attribute set must carry subms.recipe.slug");
        assertTrue(sawCategory, "summary attribute set must carry subms.recipe.category");
    }

    @Test
    @DisplayName("recorded sample count matches the per-stage entry count plus the summary pass")
    void recordedCountMatchesWorkload() {
        MetricData latency = runAndCaptureLatency();
        // Per stage: ENTRIES per-record (hot path lean attrs) + 6 percentile records + ENTRIES downsampled to <= 500
        // captured under the summary attribute set. Total over the 3 stages.
        long total = 0L;
        List<HistogramPointData> points = List.copyOf(latency.getHistogramData().getPoints());
        for (HistogramPointData p : points) total += p.getCount();

        // Hot-path contribution: 3 stages * ENTRIES.
        long hotPath = 3L * ENTRIES;
        // Summary contribution: 6 percentiles + downsampled samples (capped at 500 by the subms summary downsampler),
        // per stage, over 3 stages.
        long summaryPerStage = 6L + Math.min(ENTRIES, 500);
        long summaryTotal = 3L * summaryPerStage;

        assertEquals(hotPath + summaryTotal, total,
                "expected " + (hotPath + summaryTotal) + " records, saw " + total);
    }
}
