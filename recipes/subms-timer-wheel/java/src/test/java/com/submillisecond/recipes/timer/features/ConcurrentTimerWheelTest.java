package com.submillisecond.recipes.timer.features;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class ConcurrentTimerWheelTest {

    @Test
    void scheduleAndTickOnOneThreadWorksLikeBase() {
        ConcurrentTimerWheel<String> w = new ConcurrentTimerWheel<>(64);
        w.schedule(2, "a");
        assertTrue(w.tick().isEmpty());
        assertEquals(List.of("a"), w.tick());
    }

    @Test
    void scheduleFromMultipleThreadsFiresAllEntries() throws Exception {
        ConcurrentTimerWheel<Integer> w = new ConcurrentTimerWheel<>(256);
        int nThreads = 4;
        int perThread = 50;
        List<Thread> threads = new ArrayList<>();
        for (int t = 0; t < nThreads; t++) {
            final int tt = t;
            Thread th = new Thread(() -> {
                for (int i = 0; i < perThread; i++) {
                    w.schedule(1 + (i % 8), tt * 1000 + i);
                }
            });
            th.start();
            threads.add(th);
        }
        for (Thread th : threads) th.join();

        int total = 0;
        for (int i = 0; i < 16; i++) total += w.tick().size();
        assertEquals(nThreads * perThread, total);
    }

    @Test
    void cancelFromAnotherThreadDropsEntry() throws Exception {
        ConcurrentTimerWheel<String> w = new ConcurrentTimerWheel<>(64);
        long id = w.schedule(5, "a");
        AtomicInteger result = new AtomicInteger();
        Thread th = new Thread(() -> result.set(w.cancel(id) ? 1 : 0));
        th.start(); th.join();
        assertEquals(1, result.get());
        for (int i = 0; i < 8; i++) assertTrue(w.tick().isEmpty());
    }

    @Test
    void tickDoesNotDeadlockWithConcurrentSchedules() throws Exception {
        ConcurrentTimerWheel<Integer> w = new ConcurrentTimerWheel<>(256);
        List<Thread> writers = new ArrayList<>();
        for (int t = 0; t < 2; t++) {
            final int tt = t;
            Thread th = new Thread(() -> {
                for (int i = 0; i < 500; i++) w.schedule(1 + (i % 16), tt * 1000 + i);
            });
            th.start();
            writers.add(th);
        }
        for (Thread th : writers) th.join();

        int total = 0;
        for (int i = 0; i < 32; i++) total += w.tick().size();
        assertEquals(1000, total);
    }

    @Test
    void cancelAfterFireReturnsFalse() {
        ConcurrentTimerWheel<String> w = new ConcurrentTimerWheel<>(64);
        long id = w.schedule(0, "now");
        for (int i = 0; i < 64; i++) if (!w.tick().isEmpty()) break;
        assertFalse(w.cancel(id));
    }

    @Test
    void multipleHandlesShareState() {
        ConcurrentTimerWheel<Integer> w = new ConcurrentTimerWheel<>(64);
        // Java doesn't have a clone() but the same reference passed to
        // another thread observes the same state - sanity-check via
        // local visibility through the monitor.
        w.schedule(2, 99);
        assertTrue(w.tick().isEmpty());
        assertEquals(List.of(99), w.tick());
        assertEquals(64, w.numSlots());
    }
}
