package com.submillisecond.recipes.ratelimit;

import com.submillisecond.recipes.ratelimit.features.DistributedLimiter;
import com.submillisecond.recipes.ratelimit.features.HierarchicalLimiter;
import com.submillisecond.recipes.ratelimit.features.InMemoryBackend;
import com.submillisecond.recipes.ratelimit.features.MeteredTokenBucket;
import com.submillisecond.recipes.ratelimit.features.MetricsSnapshot;
import com.submillisecond.recipes.ratelimit.features.TestClock;
import com.submillisecond.recipes.ratelimit.features.TokenBucket;

/**
 * Sample app: a tour of {@code subms-rate-limiter} in an exchange order-entry
 * setting, base API first then each optional shape. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.ratelimit.SampleApp}
 *
 * <ul>
 *   <li>base         - throttle an order session to the venue's per-session rate, with retry-after
 *   <li>token-bucket - a weighted message budget (order actions cost different unit weights)
 *   <li>hierarchical - a desk-wide gateway cap shared across strategy sessions
 *   <li>distributed  - one per-account quota shared across order routers
 *   <li>metrics      - scrape the throttle as its own metric source
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        baseOrderSession();
        weightedMessageBudget();
        deskGatewayCap();
        perAccountQuota();
        meteredFeedThrottle();
    }

    /** Base API: burst allowance, then throttle with a retry-after backpressure hint. */
    static void baseOrderSession() {
        System.out.println("== base: order-entry session throttle ==");
        // 100 orders/sec sustained, burst allowance of 5.
        RateLimiter session = new RateLimiter(100.0, 5);
        int granted = 0;
        for (int i = 1; i <= 8; i++) {
            if (session.tryAcquire()) {
                System.out.println("  order " + i + " sent");
                granted++;
            } else {
                System.out.println("  order " + i + " throttled");
            }
        }
        System.out.println("  -> " + granted + " of 8 orders admitted (burst = 5)");
        if (granted != 5) throw new AssertionError("burst allowance admits exactly 5");

        RateLimiter.Acquire outcome = session.tryAcquireWithRetry();
        if (outcome instanceof RateLimiter.Acquire.Retry r) {
            System.out.println("  backpressure: retry after " + (r.retryAfter().toNanos() / 1000) + " us");
            if (r.retryAfter().isZero() || r.retryAfter().isNegative()) {
                throw new AssertionError("a throttled caller must get a positive wait");
            }
        } else {
            throw new AssertionError("session is saturated; a grant here would be wrong");
        }
    }

    /** token-bucket: a weighted, all-or-nothing message budget that refills up to capacity. */
    static void weightedMessageBudget() {
        System.out.println("\n== token-bucket: weighted message budget ==");
        TestClock clock = new TestClock();
        // Budget of 10 units, refilling 5 units/sec.
        TokenBucket budget = new TokenBucket(10, 5.0, clock);

        if (!budget.tryAcquire(1)) throw new AssertionError("new order costs 1 unit");
        if (!budget.tryAcquire(5)) throw new AssertionError("bulk cancel-replace costs 5 units");
        System.out.println("  after 1 + 5 units: " + budget.available() + " left");
        if (budget.available() != 4) throw new AssertionError("expected 4 units left");

        if (budget.tryAcquire(5)) throw new AssertionError("insufficient budget must reject the batch");
        if (budget.available() != 4) throw new AssertionError("a rejected batch spends nothing");

        clock.advanceMs(1_000); // 5 units/sec -> +5, capped at 10
        System.out.println("  after 1s refill: " + budget.available() + " left");
        if (!budget.tryAcquire(5)) throw new AssertionError("refilled budget admits the batch");
    }

    /** hierarchical: a parent gateway caps the aggregate below the sum of two child strategies. */
    static void deskGatewayCap() {
        System.out.println("\n== hierarchical: desk-wide gateway cap ==");
        TestClock clock = new TestClock();
        // Parent gateway admits 5 total; two child strategies could each do 10.
        HierarchicalLimiter desk = new HierarchicalLimiter(5, 0.0, 2, 10, 0.0, () -> clock);

        int sent = 0;
        for (int round = 0; round < 10; round++) {
            if (desk.tryAcquire(round % 2, 1)) sent++;
        }
        System.out.println("  strategies offered 10 orders; gateway admitted " + sent);
        if (sent != 5) throw new AssertionError("the parent caps the desk aggregate at 5");
    }

    /** distributed: two routers share one fixed-window backend, so the account quota holds across both. */
    static void perAccountQuota() {
        System.out.println("\n== distributed: per-account quota across routers ==");
        TestClock clock = new TestClock();
        InMemoryBackend shared = new InMemoryBackend();
        long windowNs = 1_000_000_000L; // 1s window, 5 orders per account
        DistributedLimiter routerA = new DistributedLimiter(shared, 5, windowNs, clock);
        DistributedLimiter routerB = new DistributedLimiter(shared, 5, windowNs, clock);

        String account = "acct-42";
        int admitted = 0;
        for (int round = 0; round < 8; round++) {
            DistributedLimiter router = (round % 2 == 0) ? routerA : routerB;
            if (router.tryAcquire(account)) admitted++;
        }
        System.out.println("  two routers, one account: " + admitted + " of 8 admitted (quota 5)");
        if (admitted != 5) throw new AssertionError("the shared quota holds across both routers");
    }

    /** metrics: the throttle is its own metric source via snapshot(). */
    static void meteredFeedThrottle() {
        System.out.println("\n== metrics: self-observing market-data throttle ==");
        TestClock clock = new TestClock();
        // 5 requests per burst, refilling 100/sec.
        MeteredTokenBucket feed = new MeteredTokenBucket(5, 100.0, clock);

        for (int i = 0; i < 8; i++) feed.tryAcquire(1);
        MetricsSnapshot s = feed.snapshot();
        System.out.println("  granted " + s.granted() + ", rejected " + s.rejected()
                + ", headroom " + s.available());
        if (s.granted() != 5 || s.rejected() != 3) throw new AssertionError("expected 5 granted, 3 rejected");

        clock.advanceMs(100); // 100/sec -> +10, capped at 5
        if (!feed.tryAcquire(1)) throw new AssertionError("the refilled feed admits again");
        MetricsSnapshot s2 = feed.snapshot();
        System.out.println("  after refill: granted " + s2.granted() + ", refills " + s2.refills());
        if (s2.granted() != 6) throw new AssertionError("expected 6 granted after refill");
        if (s2.refills() < 1) throw new AssertionError("a refill step was observed");
    }
}
