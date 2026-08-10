package com.submillisecond.recipes.ratelimit;

import com.submillisecond.recipes.ratelimit.features.DistributedLimiter;
import com.submillisecond.recipes.ratelimit.features.HierarchicalLimiter;
import com.submillisecond.recipes.ratelimit.features.InMemoryBackend;
import com.submillisecond.recipes.ratelimit.features.KeyedRateLimiter;
import com.submillisecond.recipes.ratelimit.features.MeteredTokenBucket;
import com.submillisecond.recipes.ratelimit.features.MetricsSnapshot;
import com.submillisecond.recipes.ratelimit.features.TestClock;
import com.submillisecond.recipes.ratelimit.features.TokenBucket;

import java.time.Duration;

/**
 * Sample app: an order-entry gateway that replays a fixed tape of orders
 * against a venue's published rate limits.
 *
 * <p>Everything runs on a VIRTUAL clock the app steps itself, so the printed
 * output is byte-identical on every run. A rate limiter driven by the wall
 * clock prints a different number each time, which makes it useless as a page
 * example and useless as a regression check.
 *
 * <pre>
 *   mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.ratelimit.SampleApp
 * </pre>
 *
 * <ul>
 *   <li>base        - the session throttle, its retry-after, and the planning peek
 *   <li>keyed       - a per-symbol quota inside one session
 *   <li>token-bucket - a weighted message budget that banks idle credit
 *   <li>hierarchical - a desk gateway capping two strategy sessions
 *   <li>distributed  - one per-account quota across two stateless routers
 *   <li>metrics      - the throttle as its own metric source
 * </ul>
 */
public final class SampleApp {

    /** One line of the order tape: arrival ms into the session, symbol, action, message units. */
    record Order(long atMs, String symbol, String action, long weight) {}

    /**
     * A minute of a quiet morning: a burst of new orders on the open, a heavy
     * cancel-replace, then a trickle.
     */
    static final Order[] TAPE = {
        new Order(0, "ESU5", "new", 1),
        new Order(0, "ESU5", "new", 1),
        new Order(0, "NQU5", "new", 1),
        new Order(0, "ESU5", "cancel-replace", 3),
        new Order(1, "NQU5", "new", 1),
        new Order(1, "ESU5", "new", 1),
        new Order(4, "ESU5", "new", 1),
        new Order(9, "NQU5", "cancel-replace", 3),
    };

    static final long MS = 1_000_000L;

    public static void main(String[] args) {
        sessionThrottle();
        perSymbolQuota();
        weightedMessageBudget();
        deskGatewayCap();
        perAccountQuota();
        meteredFeedThrottle();
    }

    /**
     * The venue caps this session at 1000 messages/sec with a burst of 5. Each
     * tape line is weighted, so a cancel-replace draws three permits and can be
     * refused whole. A refusal comes back with the wait to put in the venue's
     * throttle response, and the gateway peeks before it commits so it can log
     * the queue it is looking at.
     */
    static void sessionThrottle() {
        System.out.println("== session throttle: 1000 msg/sec, burst 5 ==");
        RateLimiter session = new RateLimiter(1000.0, 5);

        long sent = 0;
        long units = 0;
        for (Order o : TAPE) {
            long now = o.atMs() * MS;
            RateLimiter.Acquire outcome = session.tryAcquireWithRetryAt(now, o.weight());
            if (outcome instanceof RateLimiter.Acquire.Ok) {
                sent++;
                units += o.weight();
                System.out.printf("  t=%2dms %-5s %-14s sent%n", o.atMs(), o.symbol(), o.action());
            } else if (outcome instanceof RateLimiter.Acquire.Retry r) {
                System.out.printf("  t=%2dms %-5s %-14s throttled, retry after %d us%n",
                        o.atMs(), o.symbol(), o.action(), r.retryAfter().toNanos() / 1000L);
            } else if (outcome instanceof RateLimiter.Acquire.Unattainable u) {
                System.out.printf("  t=%2dms %-5s %-14s rejected: weight %d exceeds the burst of %d%n",
                        o.atMs(), o.symbol(), o.action(), o.weight(), u.burstCapacity());
            }
        }
        System.out.println("  -> " + sent + " of " + TAPE.length + " messages on the wire, "
                + units + " units");
        if (sent != 7 || units != 9) throw new AssertionError("expected 7 messages, 9 units");

        // Planning, not spending: how long before the session could take
        // another cancel-replace at t=9ms, and what a weight nobody can afford
        // looks like.
        Duration wait = session.timeUntilReadyAt(9 * MS, 3).orElseThrow();
        System.out.println("  next weight-3 message conforms in " + (wait.toNanos() / 1000L) + " us");
        if (wait.toNanos() != 1_000_000L) throw new AssertionError("expected a 1ms wait");
        if (session.timeUntilReadyAt(9 * MS, 6).isPresent()) {
            throw new AssertionError("weight 6 can never fit a burst of 5");
        }

        // A reconnect gets a fresh allowance from the venue.
        session.reset();
        if (!Duration.ZERO.equals(session.timeUntilReadyAt(9 * MS, 5).orElseThrow())) {
            throw new AssertionError("reset must restore the whole burst");
        }
        System.out.println("  after reconnect: the full burst of 5 is available again");
    }

    /**
     * keyed: the venue also caps each SYMBOL, so one hot instrument cannot eat
     * the whole session allowance. State per symbol is the same single TAT, so
     * the per-symbol book is one concurrent map.
     */
    static void perSymbolQuota() {
        System.out.println("\n== keyed: per-symbol quota, 1000 msg/sec each, burst 2 ==");
        KeyedRateLimiter perSymbol = new KeyedRateLimiter(1000.0, 2);

        long sent = 0;
        for (Order o : TAPE) {
            if (perSymbol.tryAcquireAt(o.atMs() * MS, o.symbol(), 1L)
                    instanceof RateLimiter.Acquire.Ok) {
                sent++;
            } else {
                System.out.printf("  t=%2dms %-5s throttled on its own quota%n",
                        o.atMs(), o.symbol());
            }
        }
        System.out.println("  -> " + sent + " admitted across " + perSymbol.size() + " symbols");
        if (sent != 7 || perSymbol.size() != 2) throw new AssertionError("expected 7 across 2 symbols");

        // Housekeeping: a symbol that has gone quiet is back at full burst
        // anyway, so dropping it costs nothing and keeps the map sized to live
        // trading.
        int evicted = perSymbol.retainActiveAt(20 * MS);
        System.out.println("  swept at t=20ms: " + evicted + " idle symbols dropped, "
                + perSymbol.size() + " live");
        if (evicted != 2 || !perSymbol.isEmpty()) throw new AssertionError("expected an empty sweep");
    }

    /**
     * token-bucket: the same weighted budget, but with a bucket's slack model -
     * credit accumulates while the session is idle, so a quiet minute is
     * followed by a legitimate spike the GCRA window would have smoothed away.
     */
    static void weightedMessageBudget() {
        System.out.println("\n== token-bucket: weighted budget that banks idle credit ==");
        TestClock clock = new TestClock();
        // 10 units of budget, refilling 5 units/sec.
        TokenBucket budget = new TokenBucket(10, 5.0, clock);

        if (!budget.tryAcquire(1)) throw new AssertionError("new order costs 1 unit");
        if (!budget.tryAcquire(5)) throw new AssertionError("bulk cancel-replace costs 5 units");
        System.out.println("  after 1 + 5 units: " + budget.available() + " left");
        if (budget.available() != 4) throw new AssertionError("expected 4 units left");

        // All-or-nothing: a batch of 5 against 4 remaining spends nothing.
        if (budget.tryAcquire(5)) throw new AssertionError("insufficient budget must reject the batch");
        if (budget.available() != 4) throw new AssertionError("a rejected batch spends nothing");

        clock.advanceMs(1_000); // +5 units, capped at 10
        System.out.println("  after 1s idle: " + budget.available() + " left");
        if (!budget.tryAcquire(5)) throw new AssertionError("banked credit admits the batch");
    }

    /**
     * hierarchical: a desk runs two strategy sessions, each rated for its own
     * flow, but the desk's single venue uplink caps the aggregate below the sum
     * so one hot strategy cannot starve the other.
     */
    static void deskGatewayCap() {
        System.out.println("\n== hierarchical: desk uplink caps two strategies ==");
        TestClock clock = new TestClock();
        // Uplink admits 5 total; each of the two strategies could do 10 alone.
        HierarchicalLimiter desk = new HierarchicalLimiter(5, 0.0, 2, 10, 0.0, () -> clock);

        int sent = 0;
        for (int round = 0; round < 10; round++) {
            if (desk.tryAcquire(round % 2, 1)) sent++;
        }
        System.out.println("  strategies offered 10 orders; uplink admitted " + sent);
        if (sent != 5) throw new AssertionError("the parent caps the desk aggregate at 5");
    }

    /**
     * distributed: an account's venue quota must hold across a fleet of
     * stateless routers. Both consult the same fixed-window counter (the Redis
     * INCR + EXPIRE shape), so the account cannot beat the cap by spraying
     * orders across routers.
     */
    static void perAccountQuota() {
        System.out.println("\n== distributed-backend: one account quota, two routers ==");
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
        System.out.println("  8 orders sprayed across 2 routers: " + admitted + " admitted (quota 5)");
        if (admitted != 5) throw new AssertionError("the shared quota holds across both routers");
    }

    /**
     * metrics: cap outbound requests to a market-data vendor and let the
     * limiter be its own metric source - grant / reject counts, refill events,
     * live headroom - with no separate observability layer.
     */
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

    private SampleApp() {}
}
