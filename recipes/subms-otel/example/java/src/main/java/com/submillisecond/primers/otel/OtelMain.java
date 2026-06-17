package com.submillisecond.primers.otel;

import com.submillisecond.otel.OtelObserver;
import com.submillisecond.otel.OtelObserverAsync;
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsPerfHarness;

import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;

import java.time.Duration;
import java.util.concurrent.TimeUnit;

/**
 * Drives the primer's headline claim end-to-end: register one
 * {@link OtelObserver} on the harness, run a workload, watch OTEL emit; then
 * the same shape with {@link OtelObserverAsync}.
 *
 * <p>The OTEL stdout exporter is a {@link StdoutMetricExporter} built into
 * this primer so the demo runs offline. Read the interleaved output:
 * percentile table from the harness, OTEL metric blocks from the exporter.
 */
public final class OtelMain {

    private OtelMain() {}

    public static void main(String[] args) {
        runSync();
        runAsync();
    }

    /** Wire {@link OtelObserver} (synchronous) and run the workload through it. */
    private static void runSync() {
        StdoutMetricExporter exporter = StdoutMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        // Long interval; we drive emission via forceFlush() explicitly.
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-primer-otel/sync");

        System.out.println("== sync OtelObserver ==");
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java")
                .withObserver(new OtelObserver(meter));
        Workload.runWorkload(h);

        SubMsBenchSummary summary = SubMsBench.summarize(h);
        SubMsBench.printSummary(summary, System.out);

        provider.forceFlush().join(5, TimeUnit.SECONDS);
        provider.close();
    }

    /** Wire {@link OtelObserverAsync} (queued, drained off the recorder thread). */
    private static void runAsync() {
        StdoutMetricExporter exporter = StdoutMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-primer-otel/async");

        System.out.println();
        System.out.println("== async OtelObserverAsync ==");
        try (OtelObserverAsync observer = new OtelObserverAsync(meter)) {
            SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java")
                    .withObserver(observer);
            Workload.runWorkload(h);

            SubMsBenchSummary summary = SubMsBench.summarize(h);
            SubMsBench.printSummary(summary, System.out);

            // close() drains the queue; do it explicitly here so the flush below sees every sample.
            observer.drainNow();
        }
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        provider.close();
    }
}
