package com.submillisecond.recipes.otel.drift;

import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.Meter;

import java.util.Objects;

/**
 * Reference-impl divergence counter. Recipes that cross-check their output against a
 * reference library call {@link #record(String, SubMsStageKind, String, String, String)}
 * when the reference's expected value differs from theirs. Increments
 * {@value #REFERENCE_DIVERGENCE_COUNTER_NAME} with attributes:
 *
 * <ul>
 *   <li>{@code subms.stage} - the bench stage where the divergence was observed.</li>
 *   <li>{@code subms.stage.kind} - {@link SubMsStageKind#asString()} of the stage.</li>
 *   <li>{@code subms.reference.kind} - reference algorithm class (e.g. {@code set_membership}).</li>
 *   <li>{@code subms.reference.expected} - the reference's value, stringified.</li>
 *   <li>{@code subms.reference.observed} - the recipe's value, stringified.</li>
 * </ul>
 *
 * <p>Counter is built lazily on first record; cloning the recorder shares the underlying
 * instrument.
 */
public final class ReferenceDivergenceRecorder {

    /** Counter surfaced to OTEL. Stable across versions. */
    public static final String REFERENCE_DIVERGENCE_COUNTER_NAME = "subms.reference.divergence";

    /** Attribute key naming the reference algorithm class. */
    public static final String REFERENCE_KIND_ATTR = "subms.reference.kind";

    /** Attribute key for the reference's expected value, stringified. */
    public static final String REFERENCE_EXPECTED_ATTR = "subms.reference.expected";

    /** Attribute key for the recipe's observed value, stringified. */
    public static final String REFERENCE_OBSERVED_ATTR = "subms.reference.observed";

    private final Meter meter;
    private volatile LongCounter counter;

    public ReferenceDivergenceRecorder(Meter meter) {
        this.meter = Objects.requireNonNull(meter, "meter");
    }

    /**
     * Record one divergence event.
     */
    public void record(
            String stage,
            SubMsStageKind kind,
            String referenceKind,
            String expected,
            String observed) {
        Objects.requireNonNull(stage, "stage");
        Objects.requireNonNull(kind, "kind");
        Objects.requireNonNull(referenceKind, "referenceKind");
        Objects.requireNonNull(expected, "expected");
        Objects.requireNonNull(observed, "observed");
        LongCounter c = counter();
        c.add(1L, divergenceAttributes(stage, kind, referenceKind, expected, observed));
    }

    /** Build the attribute set carried by every divergence increment. */
    public static Attributes divergenceAttributes(
            String stage,
            SubMsStageKind kind,
            String referenceKind,
            String expected,
            String observed) {
        return Attributes.builder()
                .put("subms.stage", stage)
                .put("subms.stage.kind", kind.asString())
                .put(REFERENCE_KIND_ATTR, referenceKind)
                .put(REFERENCE_EXPECTED_ATTR, expected)
                .put(REFERENCE_OBSERVED_ATTR, observed)
                .build();
    }

    private LongCounter counter() {
        LongCounter c = counter;
        if (c == null) {
            synchronized (this) {
                c = counter;
                if (c == null) {
                    c = meter.counterBuilder(REFERENCE_DIVERGENCE_COUNTER_NAME)
                            .setDescription("Count of detected divergences between a recipe and its reference impl")
                            .setUnit("1")
                            .build();
                    counter = c;
                }
            }
        }
        return c;
    }
}
