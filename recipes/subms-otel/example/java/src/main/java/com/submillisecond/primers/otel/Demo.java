package com.submillisecond.primers.otel;

import com.submillisecond.otel.OtelObserver;
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;

import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;

import java.time.Duration;
import java.util.concurrent.TimeUnit;

/**
 * Five-line walkthrough: build a Meter, register an {@link OtelObserver},
 * record a handful of samples, summarise. The OTEL stdout block lands as a
 * side effect of {@code SubMsBench.summarize}.
 */
public final class Demo {

    private Demo() {}

    public static void main(String[] args) {
        StdoutMetricExporter exporter = StdoutMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("subms-primer-otel/demo");

        SubMsPerfHarness h = new SubMsPerfHarness("demo", "java")
                .withObserver(new OtelObserver(meter));
        h.meta("subms.recipe.slug", Workload.RECIPE_SLUG);
        h.meta("subms.recipe.category", Workload.RECIPE_CATEGORY);

        SubMsPerfHarness.Stage put = h.stage("put", 8).withKind(SubMsStageKind.HOT_PATH);
        for (long ns : new long[]{120, 145, 130, 162, 119, 138, 151, 127}) {
            put.record(ns);
        }

        SubMsBenchSummary summary = SubMsBench.summarize(h);
        SubMsBench.printSummary(summary, System.out);

        provider.forceFlush().join(5, TimeUnit.SECONDS);
        provider.close();
    }
}
