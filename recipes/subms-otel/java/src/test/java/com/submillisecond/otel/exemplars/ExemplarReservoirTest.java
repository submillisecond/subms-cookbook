package com.submillisecond.otel.exemplars;

import com.submillisecond.otel.OtelObserver;
import com.submillisecond.otel.SubMsOtelAttributeKeys;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import io.opentelemetry.sdk.metrics.data.LongPointData;
import io.opentelemetry.sdk.metrics.data.MetricData;
import io.opentelemetry.sdk.metrics.export.PeriodicMetricReader;
import com.submillisecond.otel.testing.InMemoryMetricExporter;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class ExemplarReservoirTest {

    @Test
    void rejectsZeroOrNegativeK() {
        assertThrows(IllegalArgumentException.class, () -> new ExemplarReservoir(0));
        assertThrows(IllegalArgumentException.class, () -> new ExemplarReservoir(-3));
    }

    @Test
    void keepsSlowestK() {
        ExemplarReservoir r = new ExemplarReservoir(3);
        // UNSPECIFIED keeps the empty bucket schedule so every sample lands in the same
        // overflow bucket; the test isolates slowest-K eviction.
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.UNSPECIFIED);
        for (long ns : new long[] {1, 5, 10, 2, 3, 8, 7, 4}) {
            r.offer(ctx, ns);
        }
        List<Exemplar> snap = r.snapshot();
        assertEquals(3, snap.size());
        boolean has10 = false, has8 = false, has7 = false;
        for (Exemplar e : snap) {
            if (e.ns() == 10L) has10 = true;
            if (e.ns() == 8L) has8 = true;
            if (e.ns() == 7L) has7 = true;
        }
        assertTrue(has10 && has8 && has7);
    }

    @Test
    void capturesFullAttributeSet() {
        ExemplarReservoir r = new ExemplarReservoir(2);
        SubMsObservationCtx ctx = new SubMsObservationCtx("bloom", "java", "put", SubMsStageKind.HOT_PATH);
        assertTrue(r.offer(ctx, 100L));
        Exemplar e = r.snapshot().get(0);
        assertEquals("put", e.attributes().get(SubMsOtelAttributeKeys.KEY_STAGE));
        assertEquals("bloom", e.attributes().get(SubMsOtelAttributeKeys.KEY_WORKLOAD));
        assertEquals(SubMsStageKind.HOT_PATH.asString(),
                e.attributes().get(SubMsOtelAttributeKeys.KEY_STAGE_KIND));
    }

    @Test
    void offerReturnsFalseWhenFasterThanFloor() {
        ExemplarReservoir r = new ExemplarReservoir(2);
        // Use UNSPECIFIED so the bucket schedule is empty and every sample lands in the
        // same overflow bucket; this lets the test exercise slowest-K eviction without
        // bucket-index drift.
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.UNSPECIFIED);
        assertTrue(r.offer(ctx, 100L));
        assertTrue(r.offer(ctx, 200L));
        // Reservoir at K=2 with [100,200]; offering 50 (smaller than floor=100) returns false.
        assertFalse(r.offer(ctx, 50L));
    }

    @Test
    void clearWipesEverything() {
        ExemplarReservoir r = new ExemplarReservoir(3);
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        r.offer(ctx, 100L);
        r.offer(ctx, 200L);
        assertEquals(2, r.snapshot().size());
        r.clear();
        assertEquals(0, r.snapshot().size());
    }

    @Test
    void capacityMatchesConstructorK() {
        assertEquals(7, new ExemplarReservoir(7).capacity());
        assertEquals(ExemplarReservoir.DEFAULT_RESERVOIR_K, new ExemplarReservoir().capacity());
    }

    @Test
    void publishEmitsGaugePoints() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        Meter meter = provider.get("exemplar-test");
        ExemplarReservoir r = new ExemplarReservoir(3);
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        r.offer(ctx, 500L);
        r.offer(ctx, 1500L);
        r.offer(ctx, 2500L);
        r.publish(meter);

        provider.forceFlush().join(5, TimeUnit.SECONDS);
        MetricData gauge = exporter.getFinishedMetricItems().stream()
                .filter(m -> m.getName().equals(ExemplarReservoir.EXEMPLAR_GAUGE_NAME))
                .findFirst()
                .orElseThrow();
        assertNotNull(gauge);
        int points = 0;
        for (LongPointData p : gauge.getLongGaugeData().getPoints()) {
            points++;
            // Each point carries the bucket upper-bound + exemplar ns attrs.
            assertNotNull(p.getAttributes().get(io.opentelemetry.api.common.AttributeKey.longKey("subms.exemplar.ns")));
        }
        assertTrue(points >= 1);
    }

    @Test
    void observerWiringIncrementsKeptCounter() {
        InMemoryMetricExporter exporter = InMemoryMetricExporter.create();
        SdkMeterProvider provider = SdkMeterProvider.builder()
                .registerMetricReader(PeriodicMetricReader.builder(exporter)
                        .setInterval(Duration.ofDays(1))
                        .build())
                .build();
        ExemplarReservoir reservoir = new ExemplarReservoir(2);
        OtelObserver observer = new OtelObserver(provider.get("exemplar-test"))
                .withExemplarReservoir(reservoir);
        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        for (long ns : new long[] {10, 20, 30, 40, 50}) {
            observer.onRecord(ctx, ns);
        }
        provider.forceFlush().join(5, TimeUnit.SECONDS);
        MetricData kept = exporter.getFinishedMetricItems().stream()
                .filter(m -> m.getName().equals(OtelObserver.EXEMPLARS_KEPT_COUNTER_NAME))
                .findFirst()
                .orElseThrow();
        long total = 0L;
        for (LongPointData p : kept.getLongSumData().getPoints()) total += p.getValue();
        assertTrue(total >= 2, "expected kept-exemplars counter to fire, got " + total);
    }
}
