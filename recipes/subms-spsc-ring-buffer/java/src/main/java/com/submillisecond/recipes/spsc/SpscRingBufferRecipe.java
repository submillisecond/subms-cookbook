package com.submillisecond.recipes.spsc;

import java.util.concurrent.atomic.AtomicReference;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

/** Stages: {@code enqueue}, {@code dequeue}. */
public final class SpscRingBufferRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "spsc-ring-buffer";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        SpscRingBuffer<Long> q = new SpscRingBuffer<>(1024);
        SpscRingBuffer<Long>.Producer p = q.producer();
        SpscRingBuffer<Long>.Consumer c = q.consumer();

        // Warm-up
        for (long i = 0; i < warmup; i++) {
            while (!p.tryPush(i)) { /* spin */ }
            while (c.tryPop() == null) { /* spin */ }
        }

        AtomicReference<long[]> dqSamplesRef = new AtomicReference<>();

        Thread consumer = new Thread(() -> {
            long[] samples = new long[entries];
            long next = 0;
            while (next < entries) {
                long t0 = System.nanoTime();
                Long v = c.tryPop();
                if (v != null) {
                    long ns = System.nanoTime() - t0;
                    if (v != next) throw new AssertionError("out of order at " + next);
                    samples[(int) next] = ns;
                    next++;
                }
            }
            dqSamplesRef.set(samples);
        });
        consumer.start();

        SubMsPerfHarness.Stage enq = h.stage("enqueue", entries);
        long i = 0;
        while (i < entries) {
            long t0 = System.nanoTime();
            if (p.tryPush(i)) {
                enq.record(System.nanoTime() - t0);
                i++;
            }
        }

        try {
            consumer.join();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }
        long[] dqSamples = dqSamplesRef.get();
        SubMsPerfHarness.Stage deq = h.stage("dequeue", dqSamples.length);
        for (long ns : dqSamples) deq.record(ns);

        h.meta("capacity", Integer.toString(q.capacity()));
    }
}
