package com.submillisecond.recipes.mpsc;

import java.util.concurrent.atomic.AtomicReference;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: {@code offer}, {@code poll}. 4 producers, 1 consumer. */
public final class MpscQueueRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "mpsc-queue";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        int producers = 4;
        int perProducer = entries / producers;
        MpscQueue<Long> q = new MpscQueue<>();

        for (int i = 0; i < warmup; i++) {
            q.push((long) i);
            while (q.tryPoll() == null) {
                Thread.onSpinWait();
            }
        }
        // Contended-path warmup via the harness helper. Serial warmup
        // compiles `push()` and `tryPoll()` but doesn't expose C2 to
        // the multi-producer cache-line traffic pattern the timed loop
        // actually uses. Without this pre-pass the first 1-2k samples
        // are JIT-cold under contention, which inflates offer p99 by
        // ~3-5x. The helper drains the queue concurrently while
        // producers push so the pattern matches the timed loop.
        int warmIters = Math.max(2000, warmup);
        Thread drainer = new Thread(() -> {
            int target = producers * warmIters;
            int got = 0;
            while (got < target) {
                if (q.tryPoll() != null) got++;
                else Thread.onSpinWait();
            }
        });
        drainer.start();
        SubMsBench.contendedWarmup(producers, warmIters, (tid, i) ->
            q.push(((long) tid << 32) | i));
        try {
            drainer.join();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }

        AtomicReference<long[]> pollSamplesRef = new AtomicReference<>();
        int total = producers * perProducer;

        Thread consumer = new Thread(() -> {
            long[] s = new long[total];
            int got = 0;
            while (got < total) {
                long t0 = SubMsTimer.nanosNow();
                Long v = q.tryPoll();
                if (v != null) {
                    s[got++] = SubMsTimer.nanosNow() - t0;
                } else {
                    Thread.onSpinWait();
                }
            }
            pollSamplesRef.set(s);
        });
        consumer.start();

        Thread[] pThreads = new Thread[producers];
        long[][] offerSamples = new long[producers][];
        for (int tid = 0; tid < producers; tid++) {
            final int t = tid;
            pThreads[tid] = new Thread(() -> {
                long[] s = new long[perProducer];
                for (int i = 0; i < perProducer; i++) {
                    long t0 = SubMsTimer.nanosNow();
                    q.push(((long) t << 32) | i);
                    s[i] = SubMsTimer.nanosNow() - t0;
                }
                offerSamples[t] = s;
            });
            pThreads[tid].start();
        }

        try {
            for (Thread p : pThreads) p.join();
            consumer.join();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }

        SubMsPerfHarness.Stage offer = h.stage("offer", total).withKind(SubMsStageKind.HOT_PATH);
        for (long[] s : offerSamples) {
            for (long ns : s) offer.record(ns);
        }
        long[] pollSamples = pollSamplesRef.get();
        SubMsPerfHarness.Stage poll = h.stage("poll", pollSamples.length).withKind(SubMsStageKind.HOT_PATH);
        for (long ns : pollSamples) poll.record(ns);

        h.meta("producers", Integer.toString(producers));
    }
}
