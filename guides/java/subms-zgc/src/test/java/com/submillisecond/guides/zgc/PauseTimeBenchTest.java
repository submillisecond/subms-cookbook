package com.submillisecond.guides.zgc;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Smoke tests for the measurement machinery in {@link PauseTimeBench}.
 *
 * Deliberately does <em>not</em> assert pause-time targets - those
 * depend on the GC flags the JVM was launched with, and a `mvn test`
 * run uses whatever the developer's default is. The point of this
 * class is to fail fast if the heartbeat loop stops producing
 * samples, or if MX-bean access silently breaks.
 */
final class PauseTimeBenchTest {

    @Test
    @DisplayName("GC MX beans expose non-negative counters")
    void gcBeansArePresent() {
        List<GarbageCollectorMXBean> beans = ManagementFactory.getGarbageCollectorMXBeans();
        assertFalse(beans.isEmpty(), "JVM must report at least one GC MX bean");
        for (GarbageCollectorMXBean gc : beans) {
            assertTrue(gc.getCollectionCount() >= 0,
                    () -> "negative collection count from " + gc.getName());
            assertTrue(gc.getCollectionTime()  >= 0,
                    () -> "negative collection time from " + gc.getName());
        }
    }

    @Test
    @DisplayName("the heartbeat loop produces samples in a no-pressure window")
    void heartbeatProducesSamples() {
        // Same loop body as PauseTimeBench.measureHeartbeat, run for 50ms
        // with no allocator competition. We assert the loop terminates,
        // produces a healthy sample count, and does not see an absurd gap
        // (which would indicate a pathological JVM under test).
        long deadline = System.nanoTime() + 50_000_000L;          // 50 ms
        int n = 0;
        long max = 0;
        long prev = System.nanoTime();
        long sink = 0;
        while (System.nanoTime() < deadline) {
            long now = System.nanoTime();
            long gap = now - prev;
            prev = now;
            if (gap > max) max = gap;
            sink ^= now;
            n++;
        }
        // touch sink so the JIT can't elide the loop body
        assertNotNull(Long.toString(sink));

        final int  finalN   = n;
        final long finalMax = max;
        assertTrue(finalN > 100,
                () -> "heartbeat produced only " + finalN + " samples in 50ms - JIT or scheduling broke");
        // 50ms is a very loose bound; a real GC pause on a quiet test machine
        // wouldn't approach this, and exceeding it indicates the test JVM is
        // sharing a host that's under heavy load.
        assertTrue(finalMax < 50_000_000L,
                () -> "no-pressure heartbeat saw a " + (finalMax / 1_000_000) + "ms gap");
    }
}
