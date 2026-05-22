package com.submillisecond.primers.springboot;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;

import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Semaphore;
import java.util.concurrent.atomic.AtomicInteger;

public final class SpringBootRecipe implements SubMsRecipe {

    private static final int DEFAULT_PORT_VTHREAD  = 18181;
    private static final int DEFAULT_PORT_PLATFORM = 18182;
    // Fixed knobs for the primer bench. The /route endpoint fans out to 4 venues
    // each parking 1 ms (see OrderRoutingController). Client concurrency runs
    // ahead of the platform pool's hard cap so Tomcat's connector queue fills
    // under platform threads and the vthread variant is unbounded.
    private static final int CONCURRENCY  = 1024;
    private static final int PLATFORM_MAX = 200;

    @Override
    public String name() { return "spring-boot-virtual-threads"; }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries     = params.entries();
        int warmup      = Math.min(params.warmup(), Math.max(50, entries / 5));
        int concurrency = CONCURRENCY;
        int tomcatMax   = PLATFORM_MAX;

        Stats vt = runStage(h, "vthread",       /*virtual*/ true,  DEFAULT_PORT_VTHREAD,
                            tomcatMax, entries, concurrency, warmup);
        Stats pl = runStage(h, "platform_pool", /*virtual*/ false, DEFAULT_PORT_PLATFORM,
                            tomcatMax, entries, concurrency, warmup);

        h.meta("entries",          Integer.toString(entries));
        h.meta("concurrency",      Integer.toString(concurrency));
        h.meta("platform_max",     Integer.toString(tomcatMax));
        h.meta("vthread_wall_ms",  Long.toString(vt.wallMs));
        h.meta("platform_wall_ms", Long.toString(pl.wallMs));
        h.meta("vthread_failures",  Integer.toString(vt.failures));
        h.meta("platform_failures", Integer.toString(pl.failures));
        h.meta("wall_speedup",     String.format("%.2f", (double) pl.wallMs / Math.max(1L, vt.wallMs)));
    }

    private static Stats runStage(SubMsPerfHarness h, String stage, boolean virtual, int port,
                                  int tomcatMax, int entries, int concurrency, int warmup) {
        ConfigurableApplicationContext ctx = startApp(virtual, port, tomcatMax);
        try {
            URI url = URI.create("http://localhost:" + port + "/route?symbol=AAPL&quantity=100");
            try (ExecutorService clientExec = Executors.newVirtualThreadPerTaskExecutor()) {
                HttpClient client = HttpClient.newBuilder()
                        .executor(clientExec)
                        .connectTimeout(Duration.ofSeconds(2))
                        .build();
                HttpRequest req = HttpRequest.newBuilder(url)
                        .timeout(Duration.ofSeconds(10))
                        .GET()
                        .build();

                runRound(client, req, warmup, concurrency, /*record into*/ null);

                long[] latencies = new long[entries];
                long t0 = System.nanoTime();
                int failures = runRound(client, req, entries, concurrency, latencies);
                long wallMs = (System.nanoTime() - t0) / 1_000_000L;

                SubMsPerfHarness.Stage st = h.stage(stage, entries);
                for (long ns : latencies) st.record(ns);
                return new Stats(wallMs, failures);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new RuntimeException("bench interrupted", e);
            }
        } finally {
            ctx.close();
        }
    }

    private static ConfigurableApplicationContext startApp(boolean virtual, int port, int tomcatMax) {
        // Pass overrides as command-line --key=value so they win over
        // application.properties (which already pins port 8080 + vthreads).
        // The builder.properties(Map) path is lower precedence than
        // application.properties on the classpath.
        //
        // routing.venue-park-micros=5000 is the lever that makes this a
        // useful vthread/platform contrast: each request now blocks ~5 ms
        // on its venue fan-out, so concurrency=1024 against a 200-thread
        // platform pool actually queues at Tomcat instead of finishing
        // before contention surfaces.
        String[] cliArgs = new String[] {
                "--spring.threads.virtual.enabled=" + virtual,
                "--server.port=" + port,
                "--server.tomcat.threads.max=" + tomcatMax,
                "--routing.venue-park-micros=5000",
                "--logging.level.root=WARN",
                "--spring.main.banner-mode=off"
        };
        return new SpringApplicationBuilder(Application.class).run(cliArgs);
    }

    private static int runRound(HttpClient client, HttpRequest req, int n, int concurrency,
                                long[] latencyNanos) throws InterruptedException {
        Semaphore gate = new Semaphore(concurrency);
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
        return failures.get();
    }

    private record Stats(long wallMs, int failures) {}
}
