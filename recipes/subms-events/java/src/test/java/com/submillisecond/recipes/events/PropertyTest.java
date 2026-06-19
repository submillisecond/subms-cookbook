package com.submillisecond.recipes.events;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.concurrent.atomic.AtomicInteger;

import org.junit.jupiter.api.Test;

/** Property-based invariant tests over randomized scenarios (seeded). */
class PropertyTest {
    private static final EventLevel[] LEVELS = {EventLevel.INFO, EventLevel.WARN, EventLevel.ERROR};

    @Test
    void propSyncDeliversFullSequenceInOrder() {
        Random rng = new Random(1);
        for (int it = 0; it < 500; it++) {
            int len = rng.nextInt(20);
            List<String> topics = new ArrayList<>();
            for (int i = 0; i < len; i++) {
                topics.add("t" + rng.nextInt(5));
            }
            List<String> seen = new ArrayList<>();
            EventDispatcher bus = EventDispatcher.sync();
            bus.addListener(e -> seen.add(e.topic()));
            for (String t : topics) {
                bus.emit(Event.builder(t).build());
            }
            assertEquals(topics, seen);
        }
    }

    @Test
    void propFilterForwardsExactlyMatching() {
        Random rng = new Random(2);
        for (int it = 0; it < 500; it++) {
            EventLevel target = LEVELS[rng.nextInt(3)];
            int len = rng.nextInt(30);
            List<EventLevel> evs = new ArrayList<>();
            for (int i = 0; i < len; i++) {
                evs.add(LEVELS[rng.nextInt(3)]);
            }
            AtomicInteger cnt = new AtomicInteger();
            FilterListener f = new FilterListener(e -> e.level() == target, e -> cnt.incrementAndGet());
            EventDispatcher bus = EventDispatcher.sync();
            bus.addListener(f);
            for (EventLevel lv : evs) {
                bus.emit(Event.builder("x").level(lv).build());
            }
            long expected = evs.stream().filter(l -> l == target).count();
            assertEquals(expected, cnt.get());
        }
    }

    @Test
    void propCompositeEachChildSeesAll() {
        Random rng = new Random(3);
        for (int it = 0; it < 300; it++) {
            int k = 1 + rng.nextInt(5);
            int emits = rng.nextInt(25);
            List<AtomicInteger> counters = new ArrayList<>();
            List<EventListener> listeners = new ArrayList<>();
            for (int i = 0; i < k; i++) {
                AtomicInteger c = new AtomicInteger();
                counters.add(c);
                listeners.add(e -> c.incrementAndGet());
            }
            EventDispatcher bus = EventDispatcher.sync();
            bus.addListener(new CompositeListener(listeners));
            for (int i = 0; i < emits; i++) {
                bus.emit(Event.builder("x").build());
            }
            for (AtomicInteger c : counters) {
                assertEquals(emits, c.get());
            }
        }
    }
}
