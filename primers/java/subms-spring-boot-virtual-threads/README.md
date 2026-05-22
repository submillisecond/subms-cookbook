# Spring Boot 4 with virtual threads

A minimum-viable Spring Boot 4.0.6 app that fans out per-request to N
"venues", aggregates, returns the best quote. The point of the primer
is what happens when you flip
{@code spring.threads.virtual.enabled=true} and put concurrent load on
the endpoint.

Stack:

- Spring Boot **4.0.6** (latest stable; Spring Framework 7, Tomcat 11).
- OpenJDK 21 (Spring Boot 4 baseline is Java 17).
- Spring Boot's bundled JUnit 5 / MockMvc for the controller tests; no
  extra test deps.

```sh
mvn -q package
mvn -q test                                                      # MockMvc tests against the controller
mvn -q spring-boot:run                                           # start on :8080

# In another terminal, drive concurrent load:
java -cp target/classes \
     com.submillisecond.primers.springboot.LoadDriver \
     'http://localhost:8080/route?symbol=AAPL&quantity=100' 5000 256
```

## What the toggle actually does

```
spring.threads.virtual.enabled=true
```

Under this single setting Spring Boot 4 changes three things at
auto-configuration time:

1. **Tomcat's protocol handler** spawns a virtual thread per accepted
   request instead of borrowing one from the platform-thread pool. The
   `server.tomcat.threads.max` knob still exists but is effectively
   ignored - virtual threads aren't pooled.
2. **`@Async` and `TaskExecutor` beans** are backed by
   `Executors.newVirtualThreadPerTaskExecutor()`. Anything that fanned
   out to a `ThreadPoolTaskExecutor` before is now fanning out to a
   virtual-thread executor with no code change.
3. **Spring's blocking RestClient / RestTemplate / JDBC paths** stop
   pinning a platform thread for the duration of the IO. The blocking
   API stays blocking; the thread underneath is cheap.

The application code in `OrderRoutingController` doesn't know any of
this. It just submits child tasks to a per-request virtual-thread
executor and waits. The win compounds: the request thread is virtual,
the child fan-out threads are virtual, the JDBC / HTTP call inside
each child is virtual, nothing pins a carrier.

## The endpoint

`OrderRoutingController.route(symbol, quantity)` simulates a real
order-routing decision:

```java
try (ExecutorService perRequest = Executors.newVirtualThreadPerTaskExecutor()) {
    List<Future<VenueQuote>> calls = new ArrayList<>(fanout);
    for (int i = 0; i < fanout; i++) {
        final int venueIdx = i;
        calls.add(perRequest.submit(() -> simulateVenueCall(symbol, quantity, venueIdx, parkNanos)));
    }
    VenueQuote best = null;
    for (Future<VenueQuote> f : calls) {
        VenueQuote q = f.get();
        if (best == null || q.priceTicks() < best.priceTicks()) best = q;
    }
    return new RoutingResult(symbol, quantity, fanout, best);
}
```

The `try-with-resources` on the per-request executor matters - a slow
or hung venue must not leak threads across requests. With virtual
threads close-and-await is cheap; with a pooled platform executor it
would be too expensive to do per request.

Each "venue call" parks for `routing.venue-park-micros` (default 1 ms)
to stand in for real network IO. Real socket reads would behave the
same from the JVM's point of view (the calling thread parks waiting on
the kernel); the bench is about scheduling, not network throughput.

## LoadDriver

`LoadDriver` is a separate main class that hammers the endpoint with
concurrent HTTP requests using `java.net.http.HttpClient` (also
configured with a virtual-thread executor) and prints end-to-end
percentile latency. Three positional args: URL, request count,
concurrency.

```text
warmup: 1000 requests, concurrency 256 -> http://localhost:8080/route?symbol=AAPL&quantity=100
measure: 5000 requests, concurrency 256

requests=5000  wall=4521ms  throughput=1106 req/s
  p50    =   4.83 ms
  p99    =   8.21 ms
  p99.9  =  14.50 ms
  max    =  21.93 ms
```

Numbers are illustrative; what matters is the *shape*. With
`spring.threads.virtual.enabled=true` you should see p99 stay close
to a small multiple of (`fanout` x `venue-park-micros`) regardless
of how much concurrency you push at it. Flip the property to `false`,
restart, and the tail blows up the moment your concurrency exceeds
`server.tomcat.threads.max` - because that's the size of the pool the
requests are now serialising through.

## When this is the right tool

Spring Boot is not a low-latency framework in the traditional sense -
it's an opinionated container with a lot of conveniences. The reason
this primer exists is that with virtual threads on, the per-request
overhead (Tomcat accept, dispatcher servlet, Jackson serialisation)
becomes a small fixed cost, and the cost of *concurrency* becomes
almost free. That changes the calculus:

- For a service that fans out to a handful of downstream calls and
  returns, Spring Boot 4 + virtual threads will handle thousands of
  concurrent in-flight requests on a modest server.
- For a service that does sustained CPU work per request, virtual
  threads do nothing - your bottleneck was always cores, not threads.
  Pick a different framework or accept the throughput ceiling.

## Files

- `pom.xml` - Spring Boot 4.0.6 parent, web + test starters, the
  spring-boot-maven-plugin for the dev `:run` goal.
- `src/main/resources/application.properties` - the single `spring.threads.virtual.enabled=true`
  toggle and a couple of bookkeeping settings.
- `src/main/java/com/submillisecond/primers/springboot/Application.java`
  Spring Boot entry point.
- `src/main/java/com/submillisecond/primers/springboot/OrderRoutingController.java`
  the `/route` endpoint and the simulated venue call.
- `src/main/java/com/submillisecond/primers/springboot/LoadDriver.java`
  HTTP load driver with percentile reporting.
- `src/test/java/com/submillisecond/primers/springboot/OrderRoutingControllerTest.java`
  MockMvc black-box test against the endpoint shape.
