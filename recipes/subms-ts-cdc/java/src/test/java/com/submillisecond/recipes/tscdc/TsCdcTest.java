package com.submillisecond.recipes.tscdc;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsSeriesMetadata;

class TsCdcTest {

    private static TsObservableCollection<Double> obs() {
        return new TsObservableCollection<>();
    }

    private static long register(TsObservableCollection<Double> o, long id, String name) {
        return o.register(TsSeriesMetadata.of(id, name));
    }

    @Test
    void subscribeThenPushYieldsPushEvent() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 1, "a");
        o.push(id, 1_000, 42.5);
        assertEquals(new TsChangeEvent.Push<>(id, 1_000L, 42.5), sub.tryRecv());
        assertNull(sub.tryRecv());
    }

    @Test
    void deleteAtFiresDeleteAtEvent() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 1, "a");
        o.push(id, 10, 1.0);
        assertInstanceOf(TsChangeEvent.Push.class, sub.tryRecv());
        assertTrue(o.deleteAt(id, 10).isPresent());
        assertEquals(new TsChangeEvent.DeleteAt<>(id, 10L), sub.tryRecv());
    }

    @Test
    void deleteAtMissingPointPublishesNothing() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 1, "a");
        assertTrue(o.deleteAt(id, 999).isEmpty());
        assertNull(sub.tryRecv());
    }

    @Test
    void deleteRangeFiresDeleteRangeEvent() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 1, "a");
        o.push(id, 1, 1.0);
        o.push(id, 2, 2.0);
        o.push(id, 3, 3.0);
        sub.drain();
        assertEquals(2, o.deleteRange(id, 1, 2));
        assertEquals(new TsChangeEvent.DeleteRange<>(id, 1L, 2L), sub.tryRecv());
    }

    @Test
    void deleteRangeEmptyPublishesNothing() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 1, "a");
        assertEquals(0, o.deleteRange(id, 100, 200));
        assertNull(sub.tryRecv());
    }

    @Test
    void deregisterFiresDeregisterEvent() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        long id = register(o, 7, "a");
        o.push(id, 1, 1.0);
        sub.drain();
        assertTrue(o.deregister(id).isPresent());
        assertEquals(new TsChangeEvent.Deregister<>(7L), sub.tryRecv());
    }

    @Test
    void deregisterUnknownPublishesNothing() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        assertTrue(o.deregister(404).isEmpty());
        assertNull(sub.tryRecv());
    }

    @Test
    void twoSubscribersBothReceiveSameEvent() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> a = o.subscribe(16);
        TsSubscription<Double> b = o.subscribe(16);
        long id = register(o, 1, "s");
        o.push(id, 5, 9.0);
        TsChangeEvent<Double> want = new TsChangeEvent.Push<>(id, 5L, 9.0);
        assertEquals(want, a.tryRecv());
        assertEquals(want, b.tryRecv());
    }

    @Test
    void noSubscriberPathIsSilentAndLossless() {
        TsObservableCollection<Double> o = obs();
        long id = register(o, 1, "s");
        for (long t = 0; t < 1_000; t++) {
            o.push(id, t, (double) t);
        }
        assertEquals(0L, o.droppedEvents());
        assertEquals(0, o.subscriberCount());
        assertEquals(1_000, o.collection().get(id).orElseThrow().size());
    }

    @Test
    void ringFullDropsEventsButCollectionKeepsAllData() {
        TsObservableCollection<Double> o = obs();
        o.subscribe(4); // never drained
        long id = register(o, 1, "s");
        int total = 100;
        for (long t = 0; t < total; t++) {
            o.push(id, t, (double) t);
        }
        assertTrue(o.droppedEvents() > 0, "a full ring must drop");
        assertEquals(total, o.collection().get(id).orElseThrow().size());
    }

    @Test
    void droppedIncrementsPerFullRingPerEvent() {
        TsObservableCollection<Double> o = obs();
        o.subscribe(2);
        o.subscribe(2);
        long id = register(o, 1, "s");
        for (long t = 0; t < 10; t++) {
            o.push(id, t, (double) t);
        }
        // 10 events * 2 rings = 20 publishes, 4 fit -> 16 drops.
        assertEquals(16L, o.droppedEvents());
    }

    @Test
    void drainReturnsFifoOrder() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(64);
        long id = register(o, 1, "s");
        for (long t = 0; t < 5; t++) {
            o.push(id, t, (double) (t * 10));
        }
        List<TsChangeEvent<Double>> events = sub.drain();
        assertEquals(5, events.size());
        for (int t = 0; t < 5; t++) {
            assertEquals(new TsChangeEvent.Push<>(id, (long) t, (double) (t * 10)), events.get(t));
        }
    }

    @Test
    void drainOnEmptyRingIsEmpty() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        assertTrue(sub.drain().isEmpty());
    }

    @Test
    void readThroughReflectsMutations() {
        TsObservableCollection<Double> o = obs();
        long id = register(o, 1, "px");
        o.push(id, 1, 1.0);
        o.push(id, 2, 2.0);
        assertEquals(2, o.collection().byName("px").orElseThrow().size());
        o.deleteAt(id, 1);
        assertEquals(1, o.collection().get(id).orElseThrow().size());
    }

    @Test
    void allEventTypesInOneSession() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(64);
        long id = register(o, 3, "s");
        o.push(id, 1, 1.0);
        o.push(id, 2, 2.0);
        o.push(id, 3, 3.0);
        o.deleteAt(id, 1);
        o.deleteRange(id, 2, 3);
        o.deregister(id);
        List<TsChangeEvent<Double>> evs = sub.drain();
        assertEquals(6, evs.size());
        assertInstanceOf(TsChangeEvent.Push.class, evs.get(0));
        assertInstanceOf(TsChangeEvent.DeleteAt.class, evs.get(3));
        assertInstanceOf(TsChangeEvent.DeleteRange.class, evs.get(4));
        assertInstanceOf(TsChangeEvent.Deregister.class, evs.get(5));
        assertEquals(3L, evs.get(5).seriesId());
    }

    @Test
    void registerPublishesNothing() {
        TsObservableCollection<Double> o = obs();
        TsSubscription<Double> sub = o.subscribe(16);
        register(o, 1, "s");
        assertNull(sub.tryRecv());
        assertEquals(1, o.subscriberCount());
    }

    @Test
    void freshCollectionIsEmpty() {
        TsObservableCollection<Double> o = obs();
        assertEquals(0, o.subscriberCount());
        assertEquals(0L, o.droppedEvents());
        assertTrue(o.collection().isEmpty());
        assertFalse(o.collection().contains(1));
    }
}
