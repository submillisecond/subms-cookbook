package com.submillisecond.recipes.ratelimit;

import java.time.Duration;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.perf.SubMsTimer;

/**
 * Lock-free rate limiter using the GCRA formulation. One {@link AtomicLong}
 * holds {@code tat_ns} (theoretical arrival time of the next permit, in ns
 * since limiter creation). {@link #tryAcquire()} CAS-loops it forward.
 *
 * <p>Wait-free uncontended; under contention each attempt is one read + one
 * CAS. No mutex, no double-spend.
 *
 * <p>Thread-safety: every method is safe to call concurrently from any number
 * of threads and there is no {@code close}/lifecycle to coordinate. Share one
 * instance; there is no benefit to one per thread.
 */
public final class RateLimiter {

    private final AtomicLong tatNs = new AtomicLong(0);
    private final long periodNs;
    private final long burstNs;
    private final long originNs = SubMsTimer.nanosNow();

    /**
     * @param ratePerSec     sustained permits per second
     * @param burstCapacity  permits allowed in a burst before throttling; 0 is
     *                       floored to 1, since a zero window rejects even the
     *                       first request
     */
    public RateLimiter(double ratePerSec, long burstCapacity) {
        this.periodNs = (long) (1_000_000_000.0 / ratePerSec);
        this.burstNs = periodNs * Math.max(1L, burstCapacity);
    }

    /** Returns {@code true} if a permit is granted, {@code false} if rate is exceeded. */
    public boolean tryAcquire() {
        long now = nowNs();
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + periodNs;
            if (newTat - now > burstNs) return false;
            if (tatNs.compareAndSet(tat, newTat)) return true;
        }
    }

    /**
     * {@link #tryAcquire()} against a caller-supplied {@code now} (ns on the
     * scale {@link #nowNs()} returns) instead of the internal monotonic clock.
     * The driven-time entry point: a simulation, a replay harness or a
     * deterministic test steps {@code now} itself rather than sleeping.
     */
    public boolean tryAcquireAt(long now) {
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + periodNs;
            if (newTat - now > burstNs) return false;
            if (tatNs.compareAndSet(tat, newTat)) return true;
        }
    }

    /**
     * Outcome of {@link RateLimiter#tryAcquireWithRetry()}: {@link Ok} when a
     * permit was granted, {@link Retry} carrying how long to wait before a
     * retry will conform (an HTTP {@code Retry-After}), or
     * {@link Unattainable} when the request is larger than the burst window and
     * no wait will ever satisfy it. Under contention the retry duration is a
     * best-effort hint - another thread may take the slot first.
     */
    public sealed interface Acquire permits Acquire.Ok, Acquire.Retry, Acquire.Unattainable {
        /** A permit was granted. */
        record Ok() implements Acquire {}

        /** Rejected; wait at least {@code retryAfter} before retrying. */
        record Retry(Duration retryAfter) implements Acquire {}

        /**
         * Rejected permanently: the weight exceeds {@code burstCapacity}, so it
         * overshoots the window even from a fully idle limiter. A sizing error
         * rather than backpressure; {@code governor} calls it
         * {@code InsufficientCapacity}.
         */
        record Unattainable(long burstCapacity) implements Acquire {}

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
        return tryAcquireWithRetryAt(nowNs(), 1L);
    }

    /** {@link #tryAcquireWithRetry()} against a caller-supplied {@code now}. */
    public Acquire tryAcquireWithRetryAt(long now) {
        return tryAcquireWithRetryAt(now, 1L);
    }

    /**
     * Draw {@code n} permits at once - a weighted request, where a heavy
     * message costs more of the budget than a light one. All-or-nothing: a
     * rejected call spends nothing. A weight above {@link #burstCapacity()} can
     * never be granted; {@link #tryAcquireWithRetry(long)} reports that as a
     * typed outcome rather than a bare {@code false}.
     */
    public boolean tryAcquire(long n) {
        return tryAcquireAt(nowNs(), n);
    }

    /** {@link #tryAcquire(long)} against a caller-supplied {@code now}. */
    public boolean tryAcquireAt(long now, long n) {
        return tryAcquireWithRetryAt(now, n) instanceof Acquire.Ok;
    }

    /** {@link #tryAcquire(long)} reporting the retry-after on rejection. */
    public Acquire tryAcquireWithRetry(long n) {
        return tryAcquireWithRetryAt(nowNs(), n);
    }

    /**
     * The weighted GCRA step: a request of weight {@code n} costs {@code n}
     * periods of theoretical arrival time. {@code n == 0} is a free probe that
     * neither advances the limiter nor can be rejected.
     */
    public Acquire tryAcquireWithRetryAt(long now, long n) {
        if (n == 0L) return Acquire.OK;
        if (n < 0L) throw new IllegalArgumentException("n must be >= 0, got " + n);
        long cost = periodNs * n;
        if (cost > burstNs) return new Acquire.Unattainable(burstCapacity());
        while (true) {
            long tat = tatNs.get();
            long newTat = Math.max(now, tat) + cost;
            if (newTat - now > burstNs) {
                // Rejected: wait until the slot re-enters the burst window.
                return new Acquire.Retry(Duration.ofNanos(newTat - burstNs - now));
            }
            if (tatNs.compareAndSet(tat, newTat)) return Acquire.OK;
        }
    }

    /**
     * How long until {@code n} permits would conform, without taking them. An
     * empty {@link Optional} means {@code n} exceeds the burst capacity and no
     * wait will help. Read-only: unlike {@code tryAcquire} this never advances
     * the limiter, so a scheduler can plan against it without spending budget.
     */
    public Optional<Duration> timeUntilReady(long n) {
        return timeUntilReadyAt(nowNs(), n);
    }

    /** {@link #timeUntilReady(long)} against a caller-supplied {@code now}. */
    public Optional<Duration> timeUntilReadyAt(long now, long n) {
        if (n == 0L) return Optional.of(Duration.ZERO);
        long cost = periodNs * n;
        if (cost > burstNs) return Optional.empty();
        long tat = tatNs.get();
        long newTat = Math.max(now, tat) + cost;
        if (newTat - now > burstNs) {
            return Optional.of(Duration.ofNanos(newTat - burstNs - now));
        }
        return Optional.of(Duration.ZERO);
    }

    /**
     * Block until {@code n} permits are granted or {@code timeout} elapses,
     * whichever comes first. Returns {@code false} without sleeping when the
     * wait provably exceeds the timeout, matching Guava's
     * {@code tryAcquire(permits, timeout, unit)}.
     *
     * <p>Waiters are not queued, so this is not FIFO: several blocked callers
     * wake and race for the same slot. It sleeps by design and sits outside the
     * per-op sub-ms claim.
     */
    public boolean acquireWithin(long n, Duration timeout) throws InterruptedException {
        long deadline = System.nanoTime() + timeout.toNanos();
        while (true) {
            Acquire outcome = tryAcquireWithRetry(n);
            if (outcome instanceof Acquire.Ok) return true;
            if (outcome instanceof Acquire.Unattainable) return false;
            long wait = ((Acquire.Retry) outcome).retryAfter().toNanos();
            if (wait > deadline - System.nanoTime()) return false;
            Thread.sleep(wait / 1_000_000L, (int) (wait % 1_000_000L));
        }
    }

    /**
     * Drop all accumulated throttle state: the next {@link #burstCapacity()}
     * permits are granted immediately. For a session that reconnects and gets a
     * fresh allowance from the venue, or a test that reuses one limiter.
     */
    public void reset() {
        tatNs.set(0L);
    }

    /**
     * Nanoseconds elapsed on the limiter's own monotonic clock - the scale the
     * {@code At} methods expect, so a caller can read the clock once and reuse
     * it across several limiters.
     */
    public long nowNs() {
        return SubMsTimer.nanosNow() - originNs;
    }

    public double ratePerSec() {
        return periodNs == 0 ? Double.POSITIVE_INFINITY : 1_000_000_000.0 / periodNs;
    }

    public long burstCapacity() {
        return periodNs == 0 ? 0 : burstNs / periodNs;
    }
}
