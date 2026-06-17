package com.submillisecond.recipes.ratelimit;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stage: {@code try_acquire}. 8-way contention against one limiter. */
public final class RateLimiterRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "rate-limiter";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        RateLimiter rl = new RateLimiter(1_000_000.0, 1_000_000);
        for (int i = 0; i < warmup; i++) rl.tryAcquire();

        int threads = 8;
        int perThread = entries / threads;
        long[][] samples = new long[threads][];
        Thread[] ts = new Thread[threads];
        for (int i = 0; i < threads; i++) {
            final int tid = i;
            ts[i] = new Thread(() -> {
                long[] s = new long[perThread];
                for (int j = 0; j < perThread; j++) {
                    long t0 = SubMsTimer.nanosNow();
                    rl.tryAcquire();
                    s[j] = SubMsTimer.nanosNow() - t0;
                }
                samples[tid] = s;
            });
            ts[i].start();
        }
        try {
            for (Thread t : ts) t.join();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }

        SubMsPerfHarness.Stage stage = h.stage("try_acquire", threads * perThread).withKind(SubMsStageKind.HOT_PATH);
        for (long[] s : samples) {
            for (long ns : s) stage.record(ns);
        }
        h.meta("threads", Integer.toString(threads));
    }
}
