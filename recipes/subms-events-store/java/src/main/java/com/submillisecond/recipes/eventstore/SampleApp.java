package com.submillisecond.recipes.eventstore;

import java.util.concurrent.atomic.AtomicInteger;

import com.submillisecond.recipes.events.Event;

/**
 * Sample app: event-sourcing a trading position with {@code subms-events-store}.
 * Append the order-lifecycle events for one instrument, then rebuild the
 * aggregate (net shares + cash) two ways - a cold full {@code replay} and a live
 * {@link Projector} that only folds the new tail. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.eventstore.SampleApp}
 *
 * <p>The recipe has no optional features, so this base scenario is the whole
 * tour: append + offsets, replay/rehydrate, incremental catchUp, topic filter,
 * and a live risk subscriber.
 */
public final class SampleApp {

    /** The rebuilt read model: net shares and realized cash in cents. */
    record Position(long shares, long cashCents) {
        static final Position EMPTY = new Position(0, 0);
    }

    /** The single fold rule. Only order.filled moves the position. */
    static Position applyFill(Position pos, Event e) {
        if (!e.topic().equals("order.filled")) {
            return pos;
        }
        long qty = Long.parseLong(e.attr("qty"));
        long price = Long.parseLong(e.attr("price"));
        return switch (e.attr("side")) {
            case "buy" -> new Position(pos.shares() + qty, pos.cashCents() - qty * price);
            case "sell" -> new Position(pos.shares() - qty, pos.cashCents() + qty * price);
            default -> pos;
        };
    }

    static Event fill(String side, long qty, long price, String at) {
        return Event.builder("order.filled")
                .at(at)
                .attr("side", side)
                .attr("qty", Long.toString(qty))
                .attr("price", Long.toString(price))
                .build();
    }

    public static void main(String[] args) {
        System.out.println("== base: event-sourcing an AAPL position ==");
        EventStore store = new EventStore();

        AtomicInteger fillsSeen = new AtomicInteger();
        store.subscribe(e -> {
            if (e.topic().equals("order.filled")) {
                fillsSeen.incrementAndGet();
            }
        });

        Event[] lifecycle = {
            Event.builder("order.submitted").at("09:30:00").attr("id", "A1").attr("side", "buy").attr("qty", "100").build(),
            fill("buy", 100, 18_800, "09:30:01"),
            Event.builder("order.submitted").at("09:45:00").attr("id", "A2").attr("side", "buy").attr("qty", "50").build(),
            fill("buy", 50, 19_010, "09:45:02"),
            Event.builder("order.canceled").at("10:00:00").attr("id", "A3").build(),
        };
        for (Event e : lifecycle) {
            long offset = store.append(e);
            System.out.println("  offset " + offset + " <- " + store.get(offset).orElseThrow().topic());
        }

        Position position = Projector.replay(store, Position.EMPTY, SampleApp::applyFill);
        System.out.println("  replay -> " + position.shares() + " shares, cash " + position.cashCents() + " cents");
        if (position.shares() != 150) throw new AssertionError("100 + 50 bought");
        if (position.cashCents() != -(100 * 18_800 + 50 * 19_010)) throw new AssertionError("cash");

        Projector<Position> live = new Projector<>(Position.EMPTY);
        live.catchUp(store, SampleApp::applyFill);
        if (!live.state().equals(position)) throw new AssertionError("catchUp agrees with a full replay");
        System.out.println("  projector caught up through offset " + live.position());

        store.append(fill("sell", 40, 19_500, "10:15:00"));
        live.catchUp(store, SampleApp::applyFill);
        System.out.println("  after sell -> " + live.state().shares() + " shares, cash " + live.state().cashCents() + " cents");
        if (live.state().shares() != 110) throw new AssertionError("150 - 40 sold");
        if (live.position() != store.size()) throw new AssertionError("consumed the whole log");

        long fillCount = store.byTopic("order.filled").size();
        System.out.println("  " + fillCount + " fills in the log");
        if (fillCount != 3) throw new AssertionError("three fills");

        if (fillsSeen.get() != 3) throw new AssertionError("risk desk saw all fills");
        System.out.println("  risk desk observed " + fillsSeen.get() + " fills live");
    }
}
