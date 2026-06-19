package com.submillisecond.otel.state;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.Map;

import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventDispatcher;
import com.submillisecond.recipes.events.EventLevel;

import io.opentelemetry.sdk.metrics.SdkMeterProvider;
import org.junit.jupiter.api.Test;

class StateTest {

    @Test
    void recorderRecordsTransitions() {
        try (SdkMeterProvider provider = SdkMeterProvider.builder().build()) {
            StateTransitionRecorder r =
                    new StateTransitionRecorder(provider.get("test"), "subms.test.transition", "test");
            assertDoesNotThrow(() -> {
                r.record("overall", "UP", "DOWN");
                r.record("db", "UP", "DEGRADED", Map.of("reason", "timeout"));
            });
        }
    }

    @Test
    void bridgeForwardsEvents() {
        try (SdkMeterProvider provider = SdkMeterProvider.builder().build()) {
            OtelEventBridge bridge = new OtelEventBridge(provider.get("test"));
            assertEquals("otel", bridge.name());
            assertDoesNotThrow(() -> {
                bridge.forward(Event.transition("subms.health.status", EventLevel.ERROR, "db", "UP", "DOWN"));
                bridge.forward(Event.builder("plain").build());
            });
        }
    }

    @Test
    void bridgePlugsIntoDispatcher() {
        try (SdkMeterProvider provider = SdkMeterProvider.builder().build()) {
            EventDispatcher bus = EventDispatcher.sync();
            bus.addBridge(new OtelEventBridge(provider.get("test")));
            assertDoesNotThrow(() -> bus.emit(Event.builder("x").build()));
            assertEquals(1, bus.listenerCount());
        }
    }

    @Test
    void defaultConstructorsUseGlobalMeter() {
        assertDoesNotThrow(() -> {
            new StateTransitionRecorder("subms.test.t2", "d").record("s", "A", "B");
            new OtelEventBridge().forward(Event.builder("y").build());
        });
    }
}
