package com.submillisecond.recipes.events;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Sample app: a tour of {@code subms-events} on a trading-domain event bus. One
 * sync dispatcher fans each order-lifecycle event out to a risk handler, a
 * ledger, and a notifications gate, with an {@link EventBridge} audit sink
 * capturing the wire JSON. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.events.SampleApp}
 */
public final class SampleApp {

    public static void main(String[] args) {
        System.out.println("== trading order bus: one emit, four handlers ==");

        AtomicLong filledNotional = new AtomicLong();
        List<String> ledger = new ArrayList<>();
        List<String> alerts = new ArrayList<>();
        List<String> audit = new ArrayList<>();

        EventDispatcher bus = EventDispatcher.sync(); // inline dispatch, no background thread

        // Risk: accumulate filled notional so a desk limit can be checked inline.
        bus.addListener(e -> {
            if (e.topic().equals("order.filled")) {
                String n = e.attr("notional");
                if (n != null) {
                    filledNotional.addAndGet(Long.parseLong(n));
                }
            }
        });

        // Ledger: record every event by topic.
        bus.addListener(e -> ledger.add(e.topic()));

        // Notifications: only fills reach the trader, via a predicate gate.
        bus.addListener(new FilterListener(
                e -> e.topic().equals("order.filled"),
                e -> alerts.add(e.attr("id") == null ? "?" : e.attr("id"))));

        // Audit: capture the wire JSON of everything that flows.
        bus.addBridge(new AuditLog(audit));

        List<Event> events = List.of(
                Event.builder("order.accepted").level(EventLevel.INFO)
                        .attr("id", "A-1").attr("symbol", "AAPL").build(),
                Event.builder("order.filled").level(EventLevel.INFO)
                        .attr("id", "A-1").attr("symbol", "AAPL").attr("notional", "25000").build(),
                Event.builder("order.filled").level(EventLevel.INFO)
                        .attr("id", "B-7").attr("symbol", "MSFT").attr("notional", "40000").build(),
                Event.transition("order.cancelled", EventLevel.WARN, "C-3", "WORKING", "CANCELLED"));
        for (Event e : events) {
            System.out.println("  emit " + e.topic());
            bus.emit(e);
        }

        System.out.println("  risk: filled notional = " + filledNotional.get());
        System.out.println("  ledger: " + ledger);
        System.out.println("  alerts (fills only): " + alerts);
        System.out.println("  audit lines captured: " + audit.size());

        if (filledNotional.get() != 65_000) {
            throw new AssertionError("risk summed both fills");
        }
        if (ledger.size() != 4) {
            throw new AssertionError("ledger saw every event");
        }
        if (alerts.size() != 2) {
            throw new AssertionError("only the two fills alerted");
        }
        if (audit.size() != 4) {
            throw new AssertionError("audit bridge saw every event");
        }
    }

    /** Forwards each event's deterministic JSON to an in-memory audit log. */
    static final class AuditLog implements EventBridge {
        private final List<String> lines;

        AuditLog(List<String> lines) {
            this.lines = lines;
        }

        @Override
        public String name() {
            return "audit";
        }

        @Override
        public void forward(Event event) {
            lines.add(event.toJson());
        }
    }
}
