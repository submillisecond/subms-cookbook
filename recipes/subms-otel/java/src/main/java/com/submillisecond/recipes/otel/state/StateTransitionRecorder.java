package com.submillisecond.recipes.otel.state;

import java.util.Map;

import io.opentelemetry.api.GlobalOpenTelemetry;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.common.AttributesBuilder;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.Meter;

/**
 * Generic state-transition counter. A reusable adapter primitive for any recipe
 * that emits "X moved from state A to state B" events - e.g. subms-health status
 * flips. Records each transition with a stable {@code scope}/{@code from}/{@code to}
 * attribute set plus any extras.
 */
public final class StateTransitionRecorder {
    private final LongCounter counter;

    public StateTransitionRecorder(Meter meter, String counterName, String description) {
        this.counter = meter.counterBuilder(counterName).setDescription(description).setUnit("1").build();
    }

    public StateTransitionRecorder(String counterName, String description) {
        this(GlobalOpenTelemetry.getMeter("subms"), counterName, description);
    }

    public void record(String scope, String from, String to, Map<String, String> extra) {
        AttributesBuilder b = Attributes.builder().put("scope", scope).put("from", from).put("to", to);
        if (extra != null) {
            extra.forEach(b::put);
        }
        counter.add(1L, b.build());
    }

    public void record(String scope, String from, String to) {
        record(scope, from, to, null);
    }
}
