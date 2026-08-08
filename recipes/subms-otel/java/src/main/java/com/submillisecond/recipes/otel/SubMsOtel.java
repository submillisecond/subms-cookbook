package com.submillisecond.recipes.otel;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsStageSummary;
import com.submillisecond.perf.SubMsTimer;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.common.AttributesBuilder;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.trace.Span;
import io.opentelemetry.api.trace.Tracer;
import io.opentelemetry.context.Context;
import io.opentelemetry.context.Scope;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * Bridge between the subms perf harness and the OpenTelemetry Java SDK.
 *
 * <p>Provides three static entry points:
 * <ul>
 *   <li>{@link #exportSummary(SubMsBenchSummary, Meter)} walks every stage in a {@link SubMsBenchSummary} and records
 *   the six headline percentiles plus the downsampled samples into a single {@link DoubleHistogram} named
 *   {@value #HISTOGRAM_NAME}, dimensioned by the {@code subms.stage} attribute (and the rest of the semantic-convention
 *   set drawn from {@code inputs} + {@code meta}).</li>
 *   <li>{@link #exportTimer(SubMsTimer, Tracer)} walks the checkpoints of a {@link SubMsTimer} and emits a parent span
 *   with one child span per checkpoint.</li>
 *   <li>{@link #histogramBoundaries(SubMsStageKind)} returns kind-aware explicit bucket boundaries (in seconds, per
 *   OTEL convention) for callers wiring custom views.</li>
 * </ul>
 *
 * <p>One histogram instrument is emitted: {@value #HISTOGRAM_NAME}, unit {@value #HISTOGRAM_UNIT}. Values are recorded
 * as {@code double} seconds (ns / 1e9), matching the Rust sibling.
 */
public final class SubMsOtel {

    /** Canonical name for the per-op latency histogram. Consumers filter on this in Grafana / dashboards. */
    public static final String HISTOGRAM_NAME = "subms.latency";

    /** Canonical unit. Seconds is OTEL convention for duration measurements; the bridge emits ns / 1e9 to satisfy it. */
    public static final String HISTOGRAM_UNIT = "s";

    private SubMsOtel() {}

    /**
     * Env-driven one-line bootstrap. Equivalent to
     * {@link com.submillisecond.recipes.otel.autoconfig.SubMsOtelBootstrap#autoConfigure()}. Returns a
     * {@link com.submillisecond.recipes.otel.autoconfig.SubMsOtelAutoConfig} carrying the wired Meter, Tracer,
     * SubMsObserver, and providers.
     */
    public static com.submillisecond.recipes.otel.autoconfig.SubMsOtelAutoConfig autoConfigure() {
        return com.submillisecond.recipes.otel.autoconfig.SubMsOtelBootstrap.autoConfigure();
    }

    /**
     * Walk every stage in {@code summary} and record its six headline percentiles plus its downsampled samples into the
     * single {@link DoubleHistogram} named {@value #HISTOGRAM_NAME} (unit {@value #HISTOGRAM_UNIT}, ns / 1e9).
     *
     * <p>Attributes attached to every recorded point come from the semantic-convention table: {@code subms.workload},
     * {@code subms.lang}, {@code subms.stage}, {@code subms.stage.kind} plus whichever of {@code subms.recipe.slug},
     * {@code subms.recipe.category}, {@code subms.workload.feature}, {@code subms.workload.entries},
     * {@code subms.workload.seed}, {@code subms.host}, {@code subms.hardware.tier}, and {@code subms.crate.version}
     * resolve from the summary's {@code inputs} + {@code meta} maps.
     */
    public static void exportSummary(SubMsBenchSummary summary, Meter meter) {
        Objects.requireNonNull(summary, "summary");
        Objects.requireNonNull(meter, "meter");
        DoubleHistogram h = histogramFor(meter);
        for (SubMsStageSummary stage : summary.stages()) {
            Attributes attrs = summaryAttributes(summary, stage.name(), null);
            h.record(stage.p50Ns() / 1e9, attrs);
            h.record(stage.p99Ns() / 1e9, attrs);
            h.record(stage.p999Ns() / 1e9, attrs);
            h.record(stage.maxNs() / 1e9, attrs);
            h.record(stage.meanNs() / 1e9, attrs);
            h.record(stage.stddevNs() / 1e9, attrs);
            stage.samplesNs().ifPresent(samples -> {
                for (long s : samples) h.record(s / 1e9, attrs);
            });
        }
    }

    /**
     * Emit a span tree for {@code timer}. A parent span named {@code subms.timer.<name>} wraps one child span per
     * checkpoint; each child carries the elapsed-since-start nanoseconds as the {@code subms.elapsed.ns} attribute and
     * the per-leg ns as {@code subms.leg.ns}.
     */
    public static void exportTimer(SubMsTimer timer, Tracer tracer) {
        Objects.requireNonNull(timer, "timer");
        Objects.requireNonNull(tracer, "tracer");
        String parentName = "subms.timer." + (timer.name().isEmpty() ? "unnamed" : timer.name());
        Span parent = tracer.spanBuilder(parentName).startSpan();
        try (Scope parentScope = parent.makeCurrent()) {
            parent.setAttribute("subms.total.ns", timer.elapsedNs());
            for (SubMsTimer.Checkpoint c : timer.checkpoints()) {
                Span child = tracer.spanBuilder(c.label())
                        .setParent(Context.current().with(parent))
                        .startSpan();
                child.setAttribute("subms.leg.ns", c.sinceLastNs());
                child.setAttribute("subms.elapsed.ns", c.sinceStartNs());
                child.setAttribute("subms.is_stop", c.isStop());
                child.end();
            }
        } finally {
            parent.end();
        }
    }

    /**
     * Kind-aware explicit bucket boundaries in seconds (OTEL convention). Use these when wiring an explicit-bucket
     * histogram view for the {@value #HISTOGRAM_NAME} instrument.
     *
     * <ul>
     *   <li>{@link SubMsStageKind#HOT_PATH}: 50 ns to 1 ms.</li>
     *   <li>{@link SubMsStageKind#BATCH_OP} and {@link SubMsStageKind#ONE_SHOT}: 1 us to 1 s.</li>
     *   <li>{@link SubMsStageKind#UNSPECIFIED}: empty (defer to OTEL default).</li>
     * </ul>
     */
    public static List<Double> histogramBoundaries(SubMsStageKind kind) {
        Objects.requireNonNull(kind, "kind");
        return switch (kind) {
            case HOT_PATH -> List.of(
                    5e-8, 1e-7, 2e-7, 5e-7, 1e-6, 2e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3);
            case BATCH_OP, ONE_SHOT -> List.of(
                    1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1.0);
            case UNSPECIFIED -> Collections.emptyList();
        };
    }

    /** Build the single shared latency histogram. */
    static DoubleHistogram histogramFor(Meter meter) {
        return meter
                .histogramBuilder(HISTOGRAM_NAME)
                .setUnit(HISTOGRAM_UNIT)
                .setDescription(
                        "subms per-op latency in seconds, dimensioned by subms.stage / subms.workload / subms.lang / subms.recipe.* attributes")
                .build();
    }

    /** Build the full attribute set for a recorded sample sourced from a post-bench {@link SubMsBenchSummary}. */
    static Attributes summaryAttributes(SubMsBenchSummary summary, String stage, SubMsStageKind kind) {
        AttributesBuilder b = Attributes.builder();
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_WORKLOAD, summary.workload());
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_LANG, summary.lang());
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_STAGE, stage);
        if (kind != null) {
            putIfPresent(b, SubMsOtelAttributeKeys.KEY_STAGE_KIND, kind.asString());
        }
        Map<String, String> meta = summary.meta();
        Map<String, String> inputs = summary.inputs();
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_RECIPE_SLUG, meta.get(SubMsOtelAttributeKeys.RECIPE_SLUG));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_RECIPE_CATEGORY, meta.get(SubMsOtelAttributeKeys.RECIPE_CATEGORY));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_WORKLOAD_FEATURE, meta.get(SubMsOtelAttributeKeys.WORKLOAD_FEATURE));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_WORKLOAD_ENTRIES, inputs.get("entries"));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_WORKLOAD_SEED, inputs.get("seed"));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_HOST, meta.get("host"));
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_HARDWARE_TIER, meta.get("hardware_tier"));
        String crateVersion = meta.get("crate_version");
        if (crateVersion == null) crateVersion = meta.get("jar_version");
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_CRATE_VERSION, crateVersion);
        return b.build();
    }

    /** Build the lean per-record attribute set carried on the hot path. */
    public static Attributes ctxAttributes(String workload, String lang, String stage, SubMsStageKind kind) {
        AttributesBuilder b = Attributes.builder();
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_WORKLOAD, workload);
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_LANG, lang);
        putIfPresent(b, SubMsOtelAttributeKeys.KEY_STAGE, stage);
        if (kind != null) {
            putIfPresent(b, SubMsOtelAttributeKeys.KEY_STAGE_KIND, kind.asString());
        }
        return b.build();
    }

    private static void putIfPresent(AttributesBuilder b, io.opentelemetry.api.common.AttributeKey<String> key, String value) {
        if (value != null && !value.isEmpty()) {
            b.put(key, value);
        }
    }
}
