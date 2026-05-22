package com.submillisecond.primers.springboot;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Drive concurrent HTTP load against a running {@link Application} and
 * print end-to-end percentile latency.
 *
 * Usage (start the app first, separate terminal):
 * <pre>
 *   mvn -q spring-boot:run
 *   java -cp target/classes com.submillisecond.primers.springboot.LoadDriver \
 *        http://localhost:8080/route?symbol=AAPL&quantity=100  5000  256
 * </pre>
 *
 * Three positional args (all optional):
 *   url           default {@code http://localhost:8080/route?symbol=AAPL&quantity=100}
 *   requests      default 5000
 *   concurrency   default 256 (concurrent in-flight HTTP requests)
 *
 * Design notes:
 *   - Uses {@link HttpClient} configured with a virtual-thread executor
 *     on the client side too. The OS would otherwise pool a handful of
 *     selector threads and serialise our 256 concurrent waits.
 *   - Latency is measured from "request started" to "response body
 *     received" - the latency a real caller observes.
 *   - Warmup window of {@code requests / 5} sends before measurement
 *     starts; Tomcat, the JIT, and the connection cache all need a
 *     moment to stabilise.
 */
public final class LoadDriver {
    private LoadDriver() {}

    public static void main(String[] args) throws Exception {
        String url         = args.length > 0 ? args[0] : "http://localhost:8080/route?symbol=AAPL&quantity=100";
        int    requests    = args.length > 1 ? Integer.parseInt(args[1]) : 5_000;
        int    concurrency = args.length > 2 ? Integer.parseInt(args[2]) : 256;

        try (ExecutorService clientExec = Executors.newVirtualThreadPerTaskExecutor()) {
            HttpClient client = HttpClient.newBuilder()
                    .executor(clientExec)
                    .connectTimeout(Duration.ofSeconds(2))
                    .build();
            HttpRequest req = HttpRequest.newBuilder(URI.create(url))
                    .timeout(Duration.ofSeconds(5))
                    .GET()
                    .build();

            preflight(client, req);

            int warmup = Math.max(50, requests / 5);
            System.out.printf("warmup: %d requests, concurrency %d -> %s%n", warmup, concurrency, url);
            runRound(client, req, warmup, concurrency, /*measure*/ null);

            long[] latencies = new long[requests];
            System.out.printf("measure: %d requests, concurrency %d%n", requests, concurrency);
            long wallStart = System.nanoTime();
            runRound(client, req, requests, concurrency, latencies);
            long wallMillis = (System.nanoTime() - wallStart) / 1_000_000;

            report(latencies, wallMillis);
        }
    }

    /** Single sanity hit so a malformed URL or a dead server fails fast. */
    private static void preflight(HttpClient client, HttpRequest req) throws Exception {
        HttpResponse<String> r = client.send(req, HttpResponse.BodyHandlers.ofString());
        if (r.statusCode() / 100 != 2) {
            throw new IllegalStateException("preflight returned HTTP " + r.statusCode() + ": " + r.body());
        }
    }

    /**
     * Submit {@code n} requests with at most {@code concurrency} in flight at any time.
     * Each request's latency lands in {@code latencyNanos[i]} if non-null; otherwise the
     * call is treated as warmup and samples are dropped.
     */
    private static void runRound(HttpClient client, HttpRequest req, int n, int concurrency,
                                 long[] latencyNanos) throws InterruptedException {
        java.util.concurrent.Semaphore gate = new java.util.concurrent.Semaphore(concurrency);
        CountDownLatch done = new CountDownLatch(n);
        AtomicInteger failures = new AtomicInteger();

        try (ExecutorService submitExec = Executors.newVirtualThreadPerTaskExecutor()) {
            for (int i = 0; i < n; i++) {
                final int idx = i;
                gate.acquire();
                submitExec.execute(() -> {
                    long t0 = System.nanoTime();
                    try {
                        HttpResponse<Void> r = client.send(req, HttpResponse.BodyHandlers.discarding());
                        if (r.statusCode() / 100 != 2) failures.incrementAndGet();
                    } catch (Exception e) {
                        failures.incrementAndGet();
                    } finally {
                        long elapsed = System.nanoTime() - t0;
                        if (latencyNanos != null) latencyNanos[idx] = elapsed;
                        gate.release();
                        done.countDown();
                    }
                });
            }
            done.await();
        }
        if (failures.get() > 0) {
            System.err.printf("warning: %d/%d requests failed (status != 2xx or exception)%n", failures.get(), n);
        }
    }

    private static void report(long[] latencyNanos, long wallMillis) {
        long[] sorted = latencyNanos.clone();
        Arrays.sort(sorted);
        long p50  = sorted[(int) (sorted.length * 0.50)];
        long p99  = sorted[(int) (sorted.length * 0.99)];
        long p999 = sorted[(int) (sorted.length * 0.999)];
        long max  = sorted[sorted.length - 1];
        double rps = sorted.length / (wallMillis / 1000.0);

        System.out.println();
        System.out.printf("requests=%d  wall=%dms  throughput=%.0f req/s%n", sorted.length, wallMillis, rps);
        System.out.printf("  p50    = %s%n", formatNanos(p50));
        System.out.printf("  p99    = %s%n", formatNanos(p99));
        System.out.printf("  p99.9  = %s%n", formatNanos(p999));
        System.out.printf("  max    = %s%n", formatNanos(max));
    }

    private static String formatNanos(long ns) {
        if (ns >= 1_000_000) return String.format("%7.3f ms", ns / 1e6);
        if (ns >= 1_000)     return String.format("%7.2f us", ns / 1e3);
        return String.format("%7d ns", ns);
    }
}
