package com.submillisecond.recipes.ratelimit;

import java.time.Duration;
import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.perf.SubMsTimer;

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
    private final long originNs = SubMsTimer.nanosNow();

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
        long now = SubMsTimer.nanosNow() - originNs;
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + periodNs;
            if (newTat - now > burstNs) return false;
            if (tatNs.compareAndSet(tat, newTat)) return true;
        }
    }

    /**
     * Outcome of {@link RateLimiter#tryAcquireWithRetry()}: {@link Ok} when a
     * permit was granted, or {@link Retry} carrying how long to wait before a
     * retry will conform (an HTTP {@code Retry-After}). Under contention the
     * duration is a best-effort hint - another thread may take the slot first.
     */
    public sealed interface Acquire permits Acquire.Ok, Acquire.Retry {
        /** A permit was granted. */
        record Ok() implements Acquire {}

        /** Rejected; wait at least {@code retryAfter} before retrying. */
        record Retry(Duration retryAfter) implements Acquire {}

        /** Shared granted instance (Ok carries no state). */
        Ok OK = new Ok();
    }

    /**
     * Like {@link #tryAcquire()}, but on rejection reports how long to wait
     * before a retry will conform - the value for an HTTP {@code Retry-After}. A
     * grant advances the limiter exactly as {@code tryAcquire} does; a rejection
     * leaves it untouched.
     */
    public Acquire tryAcquireWithRetry() {
        long now = SubMsTimer.nanosNow() - originNs;
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + periodNs;
            if (newTat - now > burstNs) {
                // Rejected: wait until the slot re-enters the burst window.
                return new Acquire.Retry(Duration.ofNanos(newTat - burstNs - now));
            }
            if (tatNs.compareAndSet(tat, newTat)) return Acquire.OK;
        }
    }

    public double ratePerSec() {
        return periodNs == 0 ? Double.POSITIVE_INFINITY : 1_000_000_000.0 / periodNs;
    }

    public long burstCapacity() {
        return periodNs == 0 ? 0 : burstNs / periodNs;
    }
}
