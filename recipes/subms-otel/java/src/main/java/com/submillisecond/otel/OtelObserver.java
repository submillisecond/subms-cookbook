package com.submillisecond.otel;

import com.submillisecond.otel.drift.ReferenceDivergenceRecorder;
import com.submillisecond.otel.exemplars.ExemplarReservoir;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.Meter;

import java.util.Objects;

/**
 * Synchronous {@link SubMsObserver} that ships every recorded sample straight into the shared OpenTelemetry
 * {@link DoubleHistogram} named {@value SubMsOtel#HISTOGRAM_NAME} (unit {@value SubMsOtel#HISTOGRAM_UNIT}). One
 * instrument per meter; stage identity is carried on the {@code subms.stage} attribute. Lazily registered on first use.
 *
 * <p>Also emits the always-on counter + drift surface:
 * <ul>
 *   <li>{@value #OPS_TOTAL_COUNTER_NAME} - +1 per {@link #onRecord(SubMsObservationCtx, long)}.</li>
 *   <li>{@value #EXEMPLARS_KEPT_COUNTER_NAME} - +1 when an attached
 *       {@link ExemplarReservoir} retains a sample.</li>
 *   <li>{@link ReferenceDivergenceRecorder#REFERENCE_DIVERGENCE_COUNTER_NAME} - explicit
 *       call via {@link #recordReferenceDivergence(String, SubMsStageKind, String, String, String)}.</li>
 * </ul>
 *
 * <p><b>Attribute set on {@link #onRecord(SubMsObservationCtx, long)}:</b> the lean set carried in the harness's
 * {@link SubMsObservationCtx}: {@code subms.workload}, {@code subms.lang}, {@code subms.stage},
 * {@code subms.stage.kind}. The full attribute set (recipe.slug, hardware.tier, ...) lands on the post-bench
 * {@link #onSummarize(SubMsBenchSummary)} pass.
 *
 * <p><b>Thread safety:</b> safe to share across recorder threads; the underlying {@link DoubleHistogram} is documented
 * as safe for concurrent {@code record} calls.
 */
public final class OtelObserver implements SubMsObserver {

    /** Counter incremented on every {@code onRecord} call. */
    public static final String OPS_TOTAL_COUNTER_NAME = "subms.bench.ops_total";

    /** Counter incremented when an attached reservoir retains a sample. */
    public static final String EXEMPLARS_KEPT_COUNTER_NAME = "subms.otel.exemplars_kept_total";

    private final Meter meter;
    private final LongCounter opsTotal;
    private final LongCounter exemplarsKept;
    private final ReferenceDivergenceRecorder drift;
    private volatile DoubleHistogram histogram;
    private volatile ExemplarReservoir exemplars;

    public OtelObserver(Meter meter) {
        this.meter = Objects.requireNonNull(meter, "meter");
        this.opsTotal = meter.counterBuilder(OPS_TOTAL_COUNTER_NAME)
                .setDescription("Per-stage operation counter; one increment per onRecord call")
                .setUnit("1")
                .build();
        this.exemplarsKept = meter.counterBuilder(EXEMPLARS_KEPT_COUNTER_NAME)
                .setDescription("Count of samples retained by an attached exemplar reservoir")
                .setUnit("1")
                .build();
        this.drift = new ReferenceDivergenceRecorder(meter);
    }

    /**
     * Attach an exemplar reservoir. Every recorded sample is offered to the reservoir;
     * retained samples increment {@value #EXEMPLARS_KEPT_COUNTER_NAME}. Returns {@code this}
     * for fluent wiring.
     */
    public OtelObserver withExemplarReservoir(ExemplarReservoir reservoir) {
        this.exemplars = reservoir;
        return this;
    }

    /** Reference to the attached reservoir, or {@code null} when none is wired. */
    public ExemplarReservoir exemplarReservoir() {
        return exemplars;
    }

    /**
     * Record a reference-impl divergence. See {@link ReferenceDivergenceRecorder} for shape.
     */
    public void recordReferenceDivergence(
            String stage,
            SubMsStageKind kind,
            String referenceKind,
            String expected,
            String observed) {
        drift.record(stage, kind, referenceKind, expected, observed);
    }

    @Override
    public void onRecord(SubMsObservationCtx ctx, long ns) {
        DoubleHistogram h = histogram();
        Attributes attrs = SubMsOtel.ctxAttributes(ctx.workload(), ctx.lang(), ctx.stage(), ctx.stageKind());
        h.record(ns / 1e9, attrs);
        opsTotal.add(1L, attrs);
        ExemplarReservoir res = exemplars;
        if (res != null && res.offer(ctx, ns)) {
            exemplarsKept.add(1L, attrs);
        }
    }

    @Override
    public void onSummarize(SubMsBenchSummary summary) {
        SubMsOtel.exportSummary(summary, meter);
        ExemplarReservoir res = exemplars;
        if (res != null) {
            res.publish(meter);
        }
    }

    private DoubleHistogram histogram() {
        DoubleHistogram h = histogram;
        if (h == null) {
            synchronized (this) {
                h = histogram;
                if (h == null) {
                    h = SubMsOtel.histogramFor(meter);
                    histogram = h;
                }
            }
        }
        return h;
    }
}
