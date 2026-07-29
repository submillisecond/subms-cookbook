package com.submillisecond.recipes.events;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        List<String> seen = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync(); // inline, no background thread
        bus.addListener(e -> seen.add(e.topic()));
        bus.emit(Event.transition("svc.status", EventLevel.ERROR, "db", "UP", "DOWN"));
        assertEquals(List.of("svc.status"), seen);
        // quickstart:end
    }

    @Test
    void orderBusFansOutToEveryHandler() {
        AtomicLong filledNotional = new AtomicLong();
        List<String> ledger = new ArrayList<>();
        List<String> alerts = new ArrayList<>();
        List<String> audit = new ArrayList<>();

        EventDispatcher bus = EventDispatcher.sync();

        bus.addListener(e -> {
            if (e.topic().equals("order.filled")) {
                String n = e.attr("notional");
                if (n != null) {
                    filledNotional.addAndGet(Long.parseLong(n));
                }
            }
        });
        bus.addListener(e -> ledger.add(e.topic()));
        bus.addListener(new FilterListener(
                e -> e.topic().equals("order.filled"),
                e -> alerts.add(e.attr("id") == null ? "?" : e.attr("id"))));
        bus.addBridge(new SampleApp.AuditLog(audit));

        bus.emit(Event.builder("order.accepted").attr("id", "A-1").build());
        bus.emit(Event.builder("order.filled").attr("id", "A-1").attr("notional", "25000").build());
        bus.emit(Event.builder("order.filled").attr("id", "B-7").attr("notional", "40000").build());
        bus.emit(Event.transition("order.cancelled", EventLevel.WARN, "C-3", "WORKING", "CANCELLED"));

        assertEquals(65_000, filledNotional.get(), "risk summed both fills");
        assertEquals(4, ledger.size(), "ledger saw every event");
        assertEquals(List.of("A-1", "B-7"), alerts, "only fills alerted, in order");
        assertEquals(4, audit.size(), "bridge saw every event");
    }

    @Test
    void auditBridgeCapturesWireJson() {
        List<String> audit = new ArrayList<>();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addBridge(new SampleApp.AuditLog(audit));

        bus.emit(Event.builder("order.filled").attr("id", "A-1").attr("notional", "25000").build());

        assertEquals(
                "{\"topic\":\"order.filled\",\"level\":\"INFO\",\"at\":\"\","
                        + "\"attributes\":{\"id\":\"A-1\",\"notional\":\"25000\"}}",
                audit.get(0));
    }

    @Test
    void filterGatesNonMatchingTopics() {
        List<String> alerts = new ArrayList<>();
        EventListener gate = new FilterListener(
                e -> e.topic().equals("order.filled"), e -> alerts.add(e.topic()));
        gate.onEvent(Event.builder("order.accepted").build());
        assertFalse(alerts.contains("order.accepted"), "non-fill blocked");
        gate.onEvent(Event.builder("order.filled").build());
        assertTrue(alerts.contains("order.filled"), "fill passed");
    }
}
