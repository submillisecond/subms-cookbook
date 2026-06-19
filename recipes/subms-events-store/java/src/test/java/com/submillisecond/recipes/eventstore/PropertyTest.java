package com.submillisecond.recipes.eventstore;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.Random;

import com.submillisecond.recipes.events.Event;
import org.junit.jupiter.api.Test;

/** An incremental Projector must agree with a full replay at every catch-up point. */
class PropertyTest {
    @Test
    void propIncrementalProjectionEqualsFullReplay() {
        Random rng = new Random(7);
        for (int it = 0; it < 300; it++) {
            EventStore store = new EventStore();
            Projector<Long> proj = new Projector<>(0L);
            int ops = rng.nextInt(40);
            for (int o = 0; o < ops; o++) {
                if (rng.nextInt(3) != 0) {
                    store.append(Event.builder("t" + rng.nextInt(4)).at("t").build());
                } else {
                    proj.catchUp(store, (n, e) -> n + 1);
                    long full = Projector.replay(store, 0L, (n, e) -> n + 1);
                    assertEquals(full, (long) proj.state());
                    assertEquals(store.size(), proj.position());
                }
            }
            proj.catchUp(store, (n, e) -> n + 1);
            assertEquals((long) store.size(), (long) proj.state());
        }
    }

    @Test
    void propOffsetsDenseAndMonotonic() {
        Random rng = new Random(13);
        for (int it = 0; it < 200; it++) {
            EventStore store = new EventStore();
            int n = rng.nextInt(50);
            for (int i = 0; i < n; i++) {
                assertEquals((long) i, store.append(Event.builder("e").at("t").build()));
            }
            assertEquals(n, store.size());
        }
    }
}
