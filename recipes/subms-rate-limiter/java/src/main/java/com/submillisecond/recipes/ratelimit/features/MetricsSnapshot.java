package com.submillisecond.recipes.ratelimit.features;

/**
 * Snapshot of per-instance metered token-bucket counters.
 *
 * @param granted   total {@code tryAcquire(n)} calls that returned true
 * @param rejected  total {@code tryAcquire(n)} calls that returned false
 * @param refills   total refill events observed (one per non-zero refill step)
 * @param available current available tokens at snapshot time
 */
public record MetricsSnapshot(long granted, long rejected, long refills, long available) {
}
