package com.submillisecond.recipes.eventstore;

import java.util.concurrent.atomic.AtomicInteger;

import com.submillisecond.recipes.eventstore.SampleApp.Position;
import com.submillisecond.recipes.events.Event;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        EventStore store = new EventStore();
        store.append(Event.builder("order.filled").at("t0").attr("qty", "100").build());
        store.append(Event.builder("order.filled").at("t1").attr("qty", "50").build());

        Projector<Long> filled = new Projector<>(0L);
        filled.catchUp(store, (n, e) -> n + 1);   // folds only the new tail
        assertEquals(2L, filled.state());
        // quickstart:end
    }

    private static EventStore seeded() {
        EventStore store = new EventStore();
        store.append(Event.builder("order.submitted").at("t").attr("id", "A1").build());
        store.append(SampleApp.fill("buy", 100, 18_800, "t"));
        store.append(SampleApp.fill("buy", 50, 19_010, "t"));
        store.append(Event.builder("order.canceled").at("t").attr("id", "A3").build());
        return store;
    }

    @Test
    void replayRebuildsPosition() {
        Position position = Projector.replay(seeded(), Position.EMPTY, SampleApp::applyFill);
        assertEquals(150, position.shares());
        assertEquals(-(100 * 18_800 + 50 * 19_010), position.cashCents());
    }

    @Test
    void projectorAgreesWithReplayAndOnlyFoldsTail() {
        EventStore store = seeded();
        Projector<Position> live = new Projector<>(Position.EMPTY);
        live.catchUp(store, SampleApp::applyFill);
        Position cold = Projector.replay(store, Position.EMPTY, SampleApp::applyFill);
        assertEquals(cold, live.state(), "incremental matches a cold replay");
        assertEquals(store.size(), live.position());

        store.append(SampleApp.fill("sell", 40, 19_500, "t"));
        live.catchUp(store, SampleApp::applyFill);
        assertEquals(110, live.state().shares());
        assertEquals(store.size(), live.position(), "consumed the whole log");
    }

    @Test
    void byTopicCountsOnlyFills() {
        EventStore store = seeded();
        assertEquals(2, store.byTopic("order.filled").size());
        assertEquals(1, store.byTopic("order.submitted").size());
    }

    @Test
    void subscriberSeesEveryFillLive() {
        AtomicInteger fills = new AtomicInteger();
        EventStore store = new EventStore();
        store.subscribe(e -> {
            if (e.topic().equals("order.filled")) {
                fills.incrementAndGet();
            }
        });
        store.append(Event.builder("order.submitted").at("t").attr("id", "A1").build());
        store.append(SampleApp.fill("buy", 100, 18_800, "t"));
        store.append(SampleApp.fill("buy", 50, 19_010, "t"));
        assertEquals(2, fills.get(), "both fills observed on append");
    }
}
