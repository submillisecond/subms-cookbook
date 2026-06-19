package com.submillisecond.otel.state;

import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventBridge;

import io.opentelemetry.api.GlobalOpenTelemetry;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.common.AttributesBuilder;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.Meter;

/**
 * A subms-events {@link EventBridge} that forwards every event to OTEL as a
 * {@code subms.events.total} counter, tagged with {@code topic}/{@code level}
 * and, when present, the {@code scope}/{@code from}/{@code to} transition
 * attributes. Plug it into any dispatcher with
 * {@code dispatcher.addBridge(new OtelEventBridge())}.
 */
public final class OtelEventBridge implements EventBridge {
    private final LongCounter counter;

    public OtelEventBridge(Meter meter) {
        this.counter = meter.counterBuilder("subms.events.total")
                .setDescription("Count of subms-events events forwarded to OTEL")
                .setUnit("1")
                .build();
    }

    public OtelEventBridge() {
        this(GlobalOpenTelemetry.getMeter("subms"));
    }

    @Override
    public String name() {
        return "otel";
    }

    @Override
    public void forward(Event event) {
        AttributesBuilder b = Attributes.builder()
                .put("topic", event.topic())
                .put("level", event.level().token());
        for (String key : new String[] {"scope", "from", "to"}) {
            String v = event.attr(key);
            if (v != null) {
                b.put(key, v);
            }
        }
        counter.add(1L, b.build());
    }
}
