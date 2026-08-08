package com.submillisecond.recipes.otel.exemplars;

import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.Attributes;

import java.util.Objects;

/**
 * One retained slow-sample exemplar: the raw nanosecond timing plus the OTEL attribute set
 * that was active when it was recorded.
 */
public final class Exemplar {

    private final String stage;
    private final SubMsStageKind kind;
    private final double bucketUpperSeconds;
    private final long ns;
    private final Attributes attributes;

    public Exemplar(String stage, SubMsStageKind kind, double bucketUpperSeconds, long ns, Attributes attributes) {
        this.stage = Objects.requireNonNull(stage, "stage");
        this.kind = Objects.requireNonNull(kind, "kind");
        this.bucketUpperSeconds = bucketUpperSeconds;
        this.ns = ns;
        this.attributes = Objects.requireNonNull(attributes, "attributes");
    }

    public String stage() {
        return stage;
    }

    public SubMsStageKind kind() {
        return kind;
    }

    public double bucketUpperSeconds() {
        return bucketUpperSeconds;
    }

    public long ns() {
        return ns;
    }

    public Attributes attributes() {
        return attributes;
    }
}
