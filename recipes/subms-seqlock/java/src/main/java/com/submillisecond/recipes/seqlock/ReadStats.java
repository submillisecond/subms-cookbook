package com.submillisecond.recipes.seqlock;

import java.util.concurrent.atomic.AtomicLong;

/** Shared read counters: one per instrumented lock, however many reader threads. */
public final class ReadStats {

    public record Snapshot(long reads, long retries, long maxRetries) {
        /** Retries per successful read; zero when nothing has been read. */
        public double retryRate() {
            return reads == 0 ? 0.0 : (double) retries / reads;
        }
    }

    private final AtomicLong reads = new AtomicLong();
    private final AtomicLong retries = new AtomicLong();
    private final AtomicLong maxRetries = new AtomicLong();

    public Snapshot snapshot() {
        return new Snapshot(reads.get(), retries.get(), maxRetries.get());
    }

    public void reset() {
        reads.set(0);
        retries.set(0);
        maxRetries.set(0);
    }

    void record(long attempts) {
        reads.incrementAndGet();
        if (attempts > 0) {
            failed(attempts);
        }
    }

    void failed(long attempts) {
        retries.addAndGet(attempts);
        maxRetries.accumulateAndGet(attempts, Math::max);
    }
}
