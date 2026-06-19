package com.submillisecond.recipes.events;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

import org.junit.jupiter.api.Test;

class DispatcherTest {

    private static void waitUntil(java.util.function.BooleanSupplier cond) {
        for (int i = 0; i < 200; i++) {
            if (cond.getAsBoolean()) {
                return;
            }
            try {
                Thread.sleep(5);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
    }

    @Test
    void syncDispatchInline() {
        AtomicInteger hits = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(e -> hits.incrementAndGet());
        bus.emit(Event.builder("a").build());
        bus.emit(Event.builder("b").build());
        assertEquals(2, hits.get());
    }

    @Test
    void asyncDispatchOffThreadFifo() {
        List<String> seen = new CopyOnWriteArrayList<>();
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.addListener(e -> seen.add(e.topic()));
        for (String t : List.of("a", "b", "c", "d")) {
            bus.emit(Event.builder(t).build());
        }
        waitUntil(() -> seen.size() >= 4);
        bus.stop();
        assertEquals(List.of("a", "b", "c", "d"), new ArrayList<>(seen));
    }

    @Test
    void asyncStressDeliversEvery() {
        AtomicInteger n = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.addListener(e -> n.incrementAndGet());
        for (int i = 0; i < 5000; i++) {
            bus.emit(Event.builder("x").build());
        }
        waitUntil(() -> n.get() >= 5000);
        bus.stop();
        assertEquals(5000, n.get());
    }

    @Test
    void compositeFanOut() {
        AtomicInteger a = new AtomicInteger();
        AtomicInteger b = new AtomicInteger();
        CompositeListener composite = new CompositeListener(List.of(e -> a.incrementAndGet(), e -> b.incrementAndGet()));
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(composite);
        bus.emit(Event.builder("x").build());
        assertEquals(1, a.get());
        assertEquals(1, b.get());
    }

    @Test
    void filterGate() {
        AtomicInteger hits = new AtomicInteger();
        FilterListener gated = new FilterListener(e -> e.level() == EventLevel.ERROR, e -> hits.incrementAndGet());
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(gated);
        bus.emit(Event.builder("a").level(EventLevel.INFO).build());
        bus.emit(Event.builder("b").level(EventLevel.ERROR).build());
        assertEquals(1, hits.get());
    }

    @Test
    void nestedFilterOverComposite() {
        AtomicInteger a = new AtomicInteger();
        AtomicInteger b = new AtomicInteger();
        CompositeListener composite = new CompositeListener(List.of(e -> a.incrementAndGet(), e -> b.incrementAndGet()));
        FilterListener gated = new FilterListener(e -> e.level() == EventLevel.ERROR, composite);
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(gated);
        bus.emit(Event.builder("ok").level(EventLevel.INFO).build());
        bus.emit(Event.builder("bad").level(EventLevel.ERROR).build());
        assertEquals(1, a.get());
        assertEquals(1, b.get());
    }

    @Test
    void filterDroppingAll() {
        AtomicInteger hits = new AtomicInteger();
        FilterListener never = new FilterListener(e -> false, e -> hits.incrementAndGet());
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(never);
        for (int i = 0; i < 10; i++) {
            bus.emit(Event.builder("x").level(EventLevel.ERROR).build());
        }
        assertEquals(0, hits.get());
    }

    @Test
    void bridgeReceivesEvents() {
        AtomicInteger n = new AtomicInteger();
        EventBridge bridge = new EventBridge() {
            @Override
            public String name() {
                return "counting";
            }

            @Override
            public void forward(Event event) {
                n.incrementAndGet();
            }
        };
        EventDispatcher bus = EventDispatcher.sync();
        bus.addBridge(bridge);
        bus.emit(Event.builder("x").build());
        bus.emit(Event.builder("y").build());
        assertEquals(2, n.get());
    }

    @Test
    void bridgeListenerNameAndFlush() {
        EventBridge b = new EventBridge() {
            @Override
            public String name() {
                return "named";
            }

            @Override
            public void forward(Event event) {}
        };
        b.flush();
        assertEquals("named", new BridgeListener(b).name());
    }

    @Test
    void emitHandleFromProducer() {
        AtomicInteger hits = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.sync();
        bus.addListener(e -> hits.incrementAndGet());
        EmitHandle h = bus.handle();
        h.emit(Event.builder("a").build());
        h.emit(Event.builder("b").build());
        assertEquals(2, hits.get());
        assertEquals(DispatchMode.SYNC, h.mode());
    }

    private static EventListener gated(CountDownLatch release, AtomicBoolean first,
            AtomicInteger entered, List<String> delivered) {
        return e -> {
            if (first.getAndSet(false)) {
                entered.incrementAndGet();
                try {
                    release.await();
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
            }
            delivered.add(e.topic());
        };
    }

    @Test
    void boundedDropNewest() {
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger entered = new AtomicInteger();
        List<String> delivered = new CopyOnWriteArrayList<>();
        EventDispatcher bus = EventDispatcher.bounded(2, OverflowPolicy.DROP_NEWEST);
        bus.addListener(gated(release, new AtomicBoolean(true), entered, delivered));
        bus.emit(Event.builder("e1").build());
        waitUntil(() -> entered.get() >= 1);
        bus.emit(Event.builder("e2").build());
        bus.emit(Event.builder("e3").build());
        bus.emit(Event.builder("e4").build()); // full -> dropped
        assertEquals(1, bus.dropped());
        release.countDown();
        waitUntil(() -> delivered.size() >= 3);
        bus.stop();
        assertEquals(List.of("e1", "e2", "e3"), new ArrayList<>(delivered));
    }

    @Test
    void boundedDropOldest() {
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger entered = new AtomicInteger();
        List<String> delivered = new CopyOnWriteArrayList<>();
        EventDispatcher bus = EventDispatcher.bounded(2, OverflowPolicy.DROP_OLDEST);
        bus.addListener(gated(release, new AtomicBoolean(true), entered, delivered));
        bus.emit(Event.builder("e1").build());
        waitUntil(() -> entered.get() >= 1);
        bus.emit(Event.builder("e2").build());
        bus.emit(Event.builder("e3").build());
        bus.emit(Event.builder("e4").build()); // evicts e2 -> [e3, e4]
        assertEquals(1, bus.dropped());
        release.countDown();
        waitUntil(() -> delivered.size() >= 3);
        bus.stop();
        assertEquals(List.of("e1", "e3", "e4"), new ArrayList<>(delivered));
    }

    @Test
    void boundedBlockDelivers() {
        AtomicInteger n = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.bounded(8, OverflowPolicy.BLOCK);
        bus.addListener(e -> n.incrementAndGet());
        for (int i = 0; i < 4; i++) {
            bus.emit(Event.builder("x").build());
        }
        waitUntil(() -> n.get() >= 4);
        bus.stop();
        assertEquals(4, n.get());
        assertEquals(0, bus.dropped());
    }

    private static void drain(java.util.function.BooleanSupplier cond) {
        for (int i = 0; i < 1000; i++) {
            if (cond.getAsBoolean()) {
                return;
            }
            try {
                Thread.sleep(5);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
    }

    private static List<Thread> produce(EmitHandle handle, int producers, long per) throws InterruptedException {
        List<Thread> ts = new ArrayList<>();
        for (int i = 0; i < producers; i++) {
            Thread t = new Thread(() -> {
                for (long k = 0; k < per; k++) {
                    handle.emit(Event.builder("x").build());
                }
            });
            ts.add(t);
            t.start();
        }
        for (Thread t : ts) {
            t.join();
        }
        return ts;
    }

    @Test
    void stressMultiProducerUnboundedNoLoss() throws InterruptedException {
        AtomicLong n = new AtomicLong();
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.addListener(e -> n.incrementAndGet());
        int producers = 8;
        long per = 150_000;
        long total = producers * per; // 1.2M ops
        produce(bus.handle(), producers, per);
        drain(() -> n.get() >= total);
        bus.stop();
        assertEquals(total, n.get()); // delivered exactly once, no loss
    }

    @Test
    void stressBoundedDropConserves() throws InterruptedException {
        AtomicLong n = new AtomicLong();
        EventDispatcher bus = EventDispatcher.bounded(1024, OverflowPolicy.DROP_NEWEST);
        bus.addListener(e -> n.incrementAndGet());
        int producers = 8;
        long per = 150_000;
        long total = producers * per;
        produce(bus.handle(), producers, per);
        drain(() -> n.get() + bus.dropped() >= total);
        bus.stop();
        // Conservation under contention: delivered + dropped == total.
        assertEquals(total, n.get() + bus.dropped());
    }

    @Test
    void modeAccessor() {
        assertEquals(DispatchMode.SYNC, EventDispatcher.sync().mode());
        assertEquals(DispatchMode.ASYNC, EventDispatcher.asynchronous().mode());
    }

    @Test
    void noListenerEmitIsNoop() {
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.emit(Event.builder("x").build());
        assertEquals(0, bus.listenerCount());
        bus.stop();
    }

    @Test
    void stopIdempotentAndEmitAfterStopSafe() {
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.addListener(e -> {});
        bus.emit(Event.builder("x").build());
        bus.stop();
        bus.stop();
        bus.emit(Event.builder("y").build());
    }

    @Test
    void syncEmitBeforeListenerNotSeen() {
        AtomicInteger hits = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.sync();
        bus.emit(Event.builder("early").build());
        bus.addListener(e -> hits.incrementAndGet());
        bus.emit(Event.builder("late").build());
        assertEquals(1, hits.get());
    }

    @Test
    void listenerCountTracks() {
        EventDispatcher bus = EventDispatcher.sync();
        assertEquals(0, bus.listenerCount());
        bus.addListener(e -> {});
        bus.addListener(e -> {});
        assertEquals(2, bus.listenerCount());
    }

    @Test
    void multipleAsyncListenersAllReceive() {
        AtomicInteger a = new AtomicInteger();
        AtomicInteger b = new AtomicInteger();
        EventDispatcher bus = EventDispatcher.asynchronous();
        bus.addListener(e -> a.incrementAndGet());
        bus.addListener(e -> b.incrementAndGet());
        bus.emit(Event.builder("x").build());
        waitUntil(() -> a.get() >= 1 && b.get() >= 1);
        bus.stop();
        assertEquals(1, a.get());
        assertEquals(1, b.get());
        assertTrue(a.get() == b.get());
        assertFalse(a.get() == 0);
    }
}
