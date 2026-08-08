package com.submillisecond.recipes.otel.exemplars;

import com.submillisecond.recipes.otel.SubMsOtel;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.common.AttributesBuilder;
import io.opentelemetry.api.metrics.LongGauge;
import io.opentelemetry.api.metrics.Meter;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Per-bucket reservoir of the slowest K samples seen for every {@code (stage, bucket)} pair.
 * Wires into {@link com.submillisecond.recipes.otel.OtelObserver} via
 * {@code OtelObserver.withExemplarReservoir(...)} - every {@code onRecord} sample is offered
 * to the reservoir, the slowest K are retained, and each retained entry carries the full
 * OTEL {@link Attributes} set that was active at record time.
 *
 * <p>Reservoir contents are published every drain pass as a synthetic gauge named
 * {@link #EXEMPLAR_GAUGE_NAME}: one observation per kept sample, with the bucket upper-bound
 * + the ns value attached as attributes.
 */
public final class ExemplarReservoir {

    /** Default K (slowest-N retention) per {@code (stage, bucket)} tuple. */
    public static final int DEFAULT_RESERVOIR_K = 5;

    /** Gauge name used when {@link #publish(Meter)} fires. */
    public static final String EXEMPLAR_GAUGE_NAME = "subms.exemplars";

    private final int k;
    private final Object lock = new Object();
    private final Map<BucketKey, List<Exemplar>> buckets = new HashMap<>();

    public ExemplarReservoir() {
        this(DEFAULT_RESERVOIR_K);
    }

    public ExemplarReservoir(int k) {
        if (k <= 0) {
            throw new IllegalArgumentException("k must be > 0");
        }
        this.k = k;
    }

    /** Per-bucket capacity. */
    public int capacity() {
        return k;
    }

    /**
     * Offer a recorded sample. Returns {@code true} if it was retained (and may have evicted
     * an existing entry).
     */
    public boolean offer(SubMsObservationCtx ctx, long ns) {
        int bucketIdx = bucketIndexFor(ctx.stageKind(), ns);
        double bucketUpper = bucketUpperSeconds(ctx.stageKind(), bucketIdx);
        Attributes attrs = SubMsOtel.ctxAttributes(
                ctx.workload(), ctx.lang(), ctx.stage(), ctx.stageKind());
        BucketKey key = new BucketKey(ctx.stage(), bucketIdx);
        synchronized (lock) {
            List<Exemplar> entries = buckets.computeIfAbsent(key, k -> new ArrayList<>());
            if (entries.size() < this.k) {
                entries.add(new Exemplar(ctx.stage(), ctx.stageKind(), bucketUpper, ns, attrs));
                entries.sort((a, b) -> Long.compare(a.ns(), b.ns()));
                return true;
            }
            if (ns > entries.get(0).ns()) {
                entries.set(0, new Exemplar(ctx.stage(), ctx.stageKind(), bucketUpper, ns, attrs));
                entries.sort((a, b) -> Long.compare(a.ns(), b.ns()));
                return true;
            }
            return false;
        }
    }

    /** Snapshot retained samples across all buckets. */
    public List<Exemplar> snapshot() {
        synchronized (lock) {
            List<Exemplar> out = new ArrayList<>();
            for (List<Exemplar> entries : buckets.values()) {
                out.addAll(entries);
            }
            return Collections.unmodifiableList(out);
        }
    }

    /** Discard everything. */
    public void clear() {
        synchronized (lock) {
            buckets.clear();
        }
    }

    /**
     * Publish every retained sample on the {@value #EXEMPLAR_GAUGE_NAME} gauge - one
     * observation per kept entry, with bucket upper-bound + ns appended as attributes.
     */
    public void publish(Meter meter) {
        LongGauge gauge = meter.gaugeBuilder(EXEMPLAR_GAUGE_NAME)
                .ofLongs()
                .setDescription("Slow-sample exemplars retained per (stage, bucket).")
                .build();
        for (Exemplar ex : snapshot()) {
            AttributesBuilder b = ex.attributes().toBuilder();
            b.put("subms.exemplar.bucket_upper_s", ex.bucketUpperSeconds());
            b.put("subms.exemplar.ns", ex.ns());
            gauge.set(ex.ns(), b.build());
        }
    }

    static int bucketIndexFor(SubMsStageKind kind, long ns) {
        double secs = ns / 1e9;
        List<Double> bounds = SubMsOtel.histogramBoundaries(kind);
        for (int i = 0; i < bounds.size(); i++) {
            if (secs <= bounds.get(i)) {
                return i;
            }
        }
        return bounds.size();
    }

    static double bucketUpperSeconds(SubMsStageKind kind, int idx) {
        List<Double> bounds = SubMsOtel.histogramBoundaries(kind);
        return (idx < bounds.size()) ? bounds.get(idx) : Double.POSITIVE_INFINITY;
    }

    private record BucketKey(String stage, int bucketIdx) {}
}
