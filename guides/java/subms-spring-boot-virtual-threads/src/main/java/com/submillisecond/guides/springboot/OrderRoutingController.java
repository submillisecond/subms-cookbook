package com.submillisecond.guides.springboot;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.locks.LockSupport;

/**
 * One endpoint that does what a real order-routing decision looks like
 * at the network layer: fan out to several "venues", wait for each to
 * respond, aggregate.
 *
 * The venues are simulated - each "venue call" parks for a small fixed
 * amount of nanoseconds, matched against realistic colocated-LAN
 * latencies. Real network IO would behave the same from the JVM's
 * point of view (the calling thread parks waiting on the socket); the
 * point of this guide is the scheduling behaviour under load, not the
 * network stack.
 *
 * The endpoint is interesting because each request spawns child tasks
 * via {@code Executors.newVirtualThreadPerTaskExecutor()}. With virtual
 * threads enabled at the Tomcat layer too, every request and every
 * child fan-out is a virtual thread - the server holds no platform
 * thread blocked on IO.
 */
@RestController
public final class OrderRoutingController {

    @Value("${routing.fanout:4}")
    private int fanout;

    @Value("${routing.venue-park-micros:1000}")
    private long venueParkMicros;

    /**
     * Simulates a request that asks N venues for a quote and returns
     * the best.
     *
     * <p>Pattern: spawn one virtual thread per venue, wait for all,
     * pick the lowest price. The virtual-thread-per-task executor is
     * closed (try-with-resources) on the way out so a slow/hung venue
     * cannot leak threads across requests.
     */
    @GetMapping("/route")
    public RoutingResult route(@RequestParam(defaultValue = "AAPL") String symbol,
                               @RequestParam(defaultValue = "100")  int quantity) {
        long parkNanos = venueParkMicros * 1_000L;

        try (ExecutorService perRequest = Executors.newVirtualThreadPerTaskExecutor()) {
            List<Future<VenueQuote>> calls = new ArrayList<>(fanout);
            for (int i = 0; i < fanout; i++) {
                final int venueIdx = i;
                calls.add(perRequest.submit(() -> simulateVenueCall(symbol, quantity, venueIdx, parkNanos)));
            }

            VenueQuote best = null;
            for (Future<VenueQuote> f : calls) {
                VenueQuote q;
                try {
                    q = f.get();
                } catch (Exception e) {
                    Thread.currentThread().interrupt();
                    throw new IllegalStateException("venue call failed", e);
                }
                if (best == null || q.priceTicks() < best.priceTicks()) best = q;
            }
            return new RoutingResult(symbol, quantity, fanout, best);
        }
    }

    /**
     * A real venue call would block on a socket read. We park for the
     * configured budget so the bench reflects scheduling behaviour
     * rather than real-network jitter.
     */
    private static VenueQuote simulateVenueCall(String symbol, int quantity, int venueIdx, long parkNanos) {
        LockSupport.parkNanos(parkNanos);
        // Deterministic per-venue jitter on price so the "best" pick is
        // not always the same venue across requests.
        long priceTicks = 19_500L + ((symbol.hashCode() & 0xFF) << 1) - venueIdx;
        return new VenueQuote("venue-" + venueIdx, priceTicks, quantity);
    }

    public record VenueQuote(String venue, long priceTicks, int sizeAvailable) {}
    public record RoutingResult(String symbol, int quantity, int venuesQueried, VenueQuote best) {}
}
