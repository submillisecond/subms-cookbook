package com.submillisecond.recipes.eventstore;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.atomic.AtomicInteger;

import com.submillisecond.recipes.events.DispatchMode;
import com.submillisecond.recipes.events.Event;
import org.junit.jupiter.api.Test;

class StoreTest {

    private static Event ev(String topic) {
        return Event.builder(topic).at("t").build();
    }

    @Test
    void appendOffsets() {
        EventStore s = new EventStore();
        assertTrue(s.isEmpty());
        assertEquals(0, s.append(ev("a")));
        assertEquals(1, s.append(ev("b")));
        assertEquals(2, s.size());
        assertFalse(s.isEmpty());
    }

    @Test
    void getAndReadFrom() {
        EventStore s = new EventStore();
        s.append(ev("a"));
        s.append(ev("b"));
        s.append(ev("c"));
        assertEquals("b", s.get(1).orElseThrow().topic());
        assertTrue(s.get(9).isEmpty());
        assertEquals(List.of("b", "c"), s.readFrom(1).stream().map(Event::topic).toList());
        assertTrue(s.readFrom(9).isEmpty());
    }

    @Test
    void byTopicFilters() {
        EventStore s = new EventStore();
        s.append(ev("x"));
        s.append(ev("y"));
        s.append(ev("x"));
        assertEquals(2, s.byTopic("x").size());
        assertEquals(0, s.byTopic("z").size());
    }

    @Test
    void emptyJson() {
        assertEquals("[]", new EventStore().toJson());
    }

    @Test
    void replayFolds() {
        EventStore s = new EventStore();
        s.append(ev("hit"));
        s.append(ev("miss"));
        s.append(ev("hit"));
        long hits = Projector.replay(s, 0L, (n, e) -> e.topic().equals("hit") ? n + 1 : n);
        assertEquals(2, hits);
    }

    @Test
    void replayEmptyInitial() {
        assertEquals(42L, Projector.replay(new EventStore(), 42L, (n, e) -> n + 1));
    }

    @Test
    void projectorIncremental() {
        EventStore s = new EventStore();
        s.append(ev("a"));
        s.append(ev("b"));
        Projector<Long> p = new Projector<>(0L);
        p.catchUp(s, (n, e) -> n + 1);
        assertEquals(2L, p.state());
        assertEquals(2L, p.position());
        s.append(ev("c"));
        p.catchUp(s, (n, e) -> n + 1);
        assertEquals(3L, p.state());
    }

    @Test
    void projectorTwiceNoop() {
        EventStore s = new EventStore();
        s.append(ev("a"));
        Projector<Long> p = new Projector<>(0L);
        p.catchUp(s, (n, e) -> n + 1);
        p.catchUp(s, (n, e) -> n + 1);
        assertEquals(1L, p.state());
    }

    @Test
    void subscribeSyncDelivers() {
        AtomicInteger hits = new AtomicInteger();
        EventStore s = new EventStore();
        s.subscribe(e -> hits.incrementAndGet());
        s.append(ev("a"));
        s.append(ev("b"));
        assertEquals(2, hits.get());
    }

    @Test
    void subscribeAsyncDelivers() {
        List<String> seen = new CopyOnWriteArrayList<>();
        EventStore s = new EventStore(DispatchMode.ASYNC);
        s.subscribe(e -> seen.add(e.topic()));
        s.append(ev("a"));
        s.append(ev("b"));
        for (int i = 0; i < 200 && seen.size() < 2; i++) {
            try {
                Thread.sleep(5);
            } catch (InterruptedException ie) {
                Thread.currentThread().interrupt();
            }
        }
        s.stop();
        assertEquals(2, seen.size());
    }

    @Test
    void crossLanguageStoreFixture() {
        EventStore s = new EventStore();
        s.append(Event.builder("user.created").at("2026-06-18T00:00:00Z").attr("id", "7").build());
        s.append(Event.builder("user.renamed").at("2026-06-18T00:00:01Z").attr("id", "7").attr("name", "ko").build());
        assertEquals(
                "[{\"topic\":\"user.created\",\"level\":\"INFO\",\"at\":\"2026-06-18T00:00:00Z\",\"attributes\":{\"id\":\"7\"}},"
                        + "{\"topic\":\"user.renamed\",\"level\":\"INFO\",\"at\":\"2026-06-18T00:00:01Z\",\"attributes\":{\"id\":\"7\",\"name\":\"ko\"}}]",
                s.toJson());
    }

    @Test
    void stressAppendReplay() {
        EventStore s = new EventStore();
        for (int i = 0; i < 10_000; i++) {
            s.append(ev("e"));
        }
        assertEquals(10_000L, Projector.replay(s, 0L, (n, e) -> n + 1));
        assertEquals(10_000, s.size());
    }
}
