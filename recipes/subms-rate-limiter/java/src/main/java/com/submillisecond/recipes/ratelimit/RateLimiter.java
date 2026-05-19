package com.submillisecond.recipes.ratelimit;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Lock-free rate limiter using the GCRA formulation. One {@link AtomicLong}
 * holds {@code tat_ns} (theoretical arrival time of the next permit, in ns
 * since limiter creation). {@link #tryAcquire()} CAS-loops it forward.
 *
 * <p>Wait-free uncontended; under contention each attempt is one read + one
 * CAS. No mutex, no double-spend.
 */
public final class RateLimiter {

    private final AtomicLong tatNs = new AtomicLong(0);
    private final long periodNs;
    private final long burstNs;
    private final long originNs = System.nanoTime();

    /**
     * @param ratePerSec     sustained permits per second
     * @param burstCapacity  permits allowed in a burst before throttling
     */
    public RateLimiter(double ratePerSec, long burstCapacity) {
        this.periodNs = (long) (1_000_000_000.0 / ratePerSec);
        this.burstNs = periodNs * Math.max(1L, burstCapacity);
    }

    /** Returns {@code true} if a permit is granted, {@code false} if rate is exceeded. */
    public boolean tryAcquire() {
        long now = System.nanoTime() - originNs;
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + periodNs;
            if (newTat - now > burstNs) return false;
            if (tatNs.compareAndSet(tat, newTat)) return true;
        }
    }

    public double ratePerSec() {
        return periodNs == 0 ? Double.POSITIVE_INFINITY : 1_000_000_000.0 / periodNs;
    }

    public long burstCapacity() {
        return periodNs == 0 ? 0 : burstNs / periodNs;
    }
}
