package com.submillisecond.recipes.ratelimit;

import com.submillisecond.recipes.ratelimit.features.DistributedLimiter;
import com.submillisecond.recipes.ratelimit.features.HierarchicalLimiter;
import com.submillisecond.recipes.ratelimit.features.InMemoryBackend;
import com.submillisecond.recipes.ratelimit.features.MeteredTokenBucket;
import com.submillisecond.recipes.ratelimit.features.MetricsSnapshot;
import com.submillisecond.recipes.ratelimit.features.TestClock;
import com.submillisecond.recipes.ratelimit.features.TokenBucket;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        RateLimiter rl = new RateLimiter(1000.0, 10); // 1000 permits/sec, burst of 10
        assertTrue(rl.tryAcquire());
        // quickstart:end
    }

    @Test
    void baseOrderSessionBurstsThenThrottles() {
        RateLimiter session = new RateLimiter(100.0, 5);
        int granted = 0;
        for (int i = 0; i < 8; i++) {
            if (session.tryAcquire()) granted++;
        }
        assertEquals(5, granted, "the burst allowance admits exactly 5 orders");

        RateLimiter.Acquire outcome = session.tryAcquireWithRetry();
        RateLimiter.Acquire.Retry retry =
                assertInstanceOf(RateLimiter.Acquire.Retry.class, outcome);
        assertTrue(retry.retryAfter().toNanos() > 0, "a throttled caller gets a positive wait");
    }

    @Test
    void tokenBucketWeightedBatchIsAtomic() {
        TestClock clock = new TestClock();
        TokenBucket budget = new TokenBucket(10, 5.0, clock);

        assertTrue(budget.tryAcquire(1));
        assertTrue(budget.tryAcquire(5));
        assertEquals(4L, budget.available());
        assertFalse(budget.tryAcquire(5), "an over-budget batch is rejected");
        assertEquals(4L, budget.available(), "a rejected batch spends nothing");

        clock.advanceMs(1_000);
        assertTrue(budget.tryAcquire(5), "refilled budget admits the batch");
    }

    @Test
    void hierarchicalParentCapsTheAggregate() {
        TestClock clock = new TestClock();
        HierarchicalLimiter desk = new HierarchicalLimiter(5, 0.0, 2, 10, 0.0, () -> clock);

        int sent = 0;
        for (int round = 0; round < 10; round++) {
            if (desk.tryAcquire(round % 2, 1)) sent++;
        }
        assertEquals(5, sent, "the parent caps the desk aggregate at 5");
    }

    @Test
    void distributedQuotaHoldsAcrossRouters() {
        TestClock clock = new TestClock();
        InMemoryBackend shared = new InMemoryBackend();
        long windowNs = 1_000_000_000L;
        DistributedLimiter routerA = new DistributedLimiter(shared, 5, windowNs, clock);
        DistributedLimiter routerB = new DistributedLimiter(shared, 5, windowNs, clock);

        int admitted = 0;
        for (int round = 0; round < 8; round++) {
            DistributedLimiter router = (round % 2 == 0) ? routerA : routerB;
            if (router.tryAcquire("acct-42")) admitted++;
        }
        assertEquals(5, admitted, "the shared quota holds across both routers");
    }

    @Test
    void metricsSnapshotCountsGrantsRejectsAndRefills() {
        TestClock clock = new TestClock();
        MeteredTokenBucket feed = new MeteredTokenBucket(5, 100.0, clock);

        for (int i = 0; i < 8; i++) feed.tryAcquire(1);
        MetricsSnapshot s = feed.snapshot();
        assertEquals(5L, s.granted());
        assertEquals(3L, s.rejected());
        assertEquals(0L, s.refills(), "no refill while the clock is still");

        clock.advanceMs(100);
        assertTrue(feed.tryAcquire(1));
        MetricsSnapshot s2 = feed.snapshot();
        assertEquals(6L, s2.granted());
        assertTrue(s2.refills() >= 1, "a refill step was observed");
    }
}
