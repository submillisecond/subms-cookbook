package com.submillisecond.recipes.ratelimit.features;

import java.time.Duration;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.ratelimit.RateLimiter;

/**
 * Per-key GCRA: one independent limiter per string key, in process.
 *
 * <p>The base {@link RateLimiter} governs one flow. A gateway usually has
 * thousands - a quota per account, per symbol, per session - and the state for
 * each is the same single {@code long} TAT, so the whole structure is a
 * concurrent map of keys to TATs.
 *
 * <p>Unlike the {@link DistributedLimiter} this is in-process and exact: it
 * keeps GCRA's smoothed outflow rather than dropping to fixed-window counters,
 * and there is no backend round trip. Reach for the distributed shape when the
 * quota has to span nodes.
 *
 * <p>Memory is the thing to watch. A keyed limiter over unbounded keys grows
 * without limit unless something sweeps it, which is what
 * {@link #retainActiveAt(long)} is for.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::KeyedRateLimiter}.
 */
public final class KeyedRateLimiter {

    private final ConcurrentHashMap<String, AtomicLong> tats = new ConcurrentHashMap<>();
    private final long periodNs;
    private final long burstNs;
    private final long originNs = SubMsTimer.nanosNow();

    /** {@code ratePerSec} and {@code burstCapacity} apply to each key independently. */
    public KeyedRateLimiter(double ratePerSec, long burstCapacity) {
        this.periodNs = (long) (1_000_000_000.0 / ratePerSec);
        this.burstNs = periodNs * Math.max(1L, burstCapacity);
    }

    /** Try one permit on {@code key}. */
    public boolean tryAcquire(String key) {
        return tryAcquire(key, 1L);
    }

    /** Try {@code n} permits on {@code key}, all or nothing. */
    public boolean tryAcquire(String key, long n) {
        return tryAcquireAt(nowNs(), key, n) instanceof RateLimiter.Acquire.Ok;
    }

    /**
     * Driven-time entry point. Returns the same typed outcome as the base
     * limiter, so a rejected caller gets its retry-after for free.
     */
    public RateLimiter.Acquire tryAcquireAt(long now, String key, long n) {
        if (n == 0L) return RateLimiter.Acquire.OK;
        if (n < 0L) throw new IllegalArgumentException("n must be >= 0, got " + n);
        long cost = periodNs * n;
        if (cost > burstNs) return new RateLimiter.Acquire.Unattainable(burstCapacity());
        while (true) {
            AtomicLong slot = tats.computeIfAbsent(key, k -> new AtomicLong(0L));
            synchronized (slot) {
                // A concurrent retainActiveAt may have unmapped this slot
                // between the lookup and the lock; writing to the orphan would
                // silently drop the grant, so re-check identity and retry.
                if (tats.get(key) != slot) continue;
                long tat = slot.get();
                long newTat = Math.max(now, tat) + cost;
                if (newTat - now > burstNs) {
                    return new RateLimiter.Acquire.Retry(Duration.ofNanos(newTat - burstNs - now));
                }
                slot.set(newTat);
                return RateLimiter.Acquire.OK;
            }
        }
    }

    /**
     * How long until {@code n} permits conform on {@code key}, without taking
     * them. Empty when {@code n} exceeds the burst capacity.
     */
    public Optional<Duration> timeUntilReadyAt(long now, String key, long n) {
        if (n == 0L) return Optional.of(Duration.ZERO);
        long cost = periodNs * n;
        if (cost > burstNs) return Optional.empty();
        AtomicLong slot = tats.get(key);
        long tat = slot == null ? 0L : slot.get();
        long newTat = Math.max(now, tat) + cost;
        if (newTat - now > burstNs) {
            return Optional.of(Duration.ofNanos(newTat - burstNs - now));
        }
        return Optional.of(Duration.ZERO);
    }

    /** Drop {@code key}'s throttle state. Returns whether it was tracked. */
    public boolean forget(String key) {
        return tats.remove(key) != null;
    }

    /** Drop every key. */
    public void clear() {
        tats.clear();
    }

    /**
     * Evict keys whose TAT has fallen behind {@code now}, returning how many
     * went.
     *
     * <p>Eviction is lossless: a key at {@code tat <= now} has drawn nothing
     * recently and its full burst is available, which is exactly the state a
     * never-seen key starts in. Call it on a housekeeping tick to keep the map
     * sized to the ACTIVE key set rather than the historical one.
     */
    public int retainActiveAt(long now) {
        int before = tats.size();
        tats.entrySet().removeIf(e -> {
            AtomicLong slot = e.getValue();
            synchronized (slot) {
                return slot.get() <= now;
            }
        });
        return before - tats.size();
    }

    /** Number of keys currently carrying state. */
    public int size() {
        return tats.size();
    }

    public boolean isEmpty() {
        return tats.isEmpty();
    }

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
