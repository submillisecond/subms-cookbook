---
lang: java
---

### Step 1 - the toggle

The whole behavioural change lives in one line of `application.properties`:

```properties
spring.threads.virtual.enabled=true
```

There is no Java code to write for it. Spring Boot 4's auto-configuration reads the property and wires Tomcat's protocol handler, `@Async`/`TaskExecutor` beans, and the blocking-IO paths to use virtual threads. Application code is unchanged.

### Step 2 - the controller

The endpoint spawns four child virtual threads per request, each simulating a venue call. The price comes back, the request picks the lowest, returns JSON. Standard Spring Boot 4 record-based response shape.

```java
@RestController
public final class OrderRoutingController {

    @Value("${routing.fanout:4}")          private int  fanout;
    @Value("${routing.venue-park-micros:1000}") private long venueParkMicros;

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
                VenueQuote q = f.get();
                if (best == null || q.priceTicks() < best.priceTicks()) best = q;
            }
            return new RoutingResult(symbol, quantity, fanout, best);
        }
    }

    public record VenueQuote(String venue, long priceTicks, int sizeAvailable) {}
    public record RoutingResult(String symbol, int quantity, int venuesQueried, VenueQuote best) {}
}
```

Three things worth noticing:

1. **Per-request executor in a try-with-resources.** A slow or hung venue must not leak threads across requests. With virtual threads this is essentially free; with a pooled platform executor doing it per request would dominate latency.
2. **Records as response shape.** Jackson serialises them out of the box in Spring Boot 4. No DTO scaffolding.
3. **`@Value` from properties, not constants.** The bench code overrides these via `application-test.properties` / `@TestPropertySource` to keep the test fast.

### Step 3 - the load driver

A separate `main` that drives concurrent HTTP using `java.net.http.HttpClient`. The client is itself configured with a virtual-thread executor so the load generator doesn't become its own bottleneck.

```java
try (ExecutorService clientExec = Executors.newVirtualThreadPerTaskExecutor()) {
    HttpClient client = HttpClient.newBuilder()
            .executor(clientExec)
            .connectTimeout(Duration.ofSeconds(2))
            .build();
    HttpRequest req = HttpRequest.newBuilder(URI.create(url))
            .timeout(Duration.ofSeconds(5))
            .GET()
            .build();

    preflight(client, req);                            // fail fast on dead server
    runRound(client, req, warmup, concurrency, null);  // discard

    long[] latencies = new long[requests];
    long wallStart = System.nanoTime();
    runRound(client, req, requests, concurrency, latencies);
    long wallMillis = (System.nanoTime() - wallStart) / 1_000_000;

    report(latencies, wallMillis);
}
```

`runRound` uses a `Semaphore(concurrency)` to bound in-flight requests and a `CountDownLatch` to join. The latency for each request is captured from "submitted" to "response body received" - the latency a real caller observes.

### Step 4 - tests

`@SpringBootTest` + `@AutoConfigureMockMvc` for the controller surface. Park budget is shrunk to keep the test fast; the production defaults live in `application.properties` and only matter under load.

```java
@SpringBootTest
@AutoConfigureMockMvc
@TestPropertySource(properties = {
        "routing.fanout=3",
        "routing.venue-park-micros=10"
})
final class OrderRoutingControllerTest {

    @Autowired private MockMvc mvc;

    @Test
    void routeReturnsExpectedShape() throws Exception {
        mvc.perform(get("/route").param("symbol", "AAPL").param("quantity", "100"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.symbol",         is("AAPL")))
                .andExpect(jsonPath("$.venuesQueried",  is(3)))
                .andExpect(jsonPath("$.best.venue",     notNullValue()));
    }
}
```

The reason for MockMvc rather than calling the controller method directly: it pins the JSON contract that the LoadDriver depends on. A casual field rename in the response records would break the load driver as well as this test, in lockstep.

### Step 5 - measure

```sh
mvn -q test                                                      # MockMvc tests, fast
mvn -q spring-boot:run                                           # start on :8080

# Separate terminal:
java -cp target/classes \
     com.submillisecond.guides.springboot.LoadDriver \
     'http://localhost:8080/route?symbol=AAPL&quantity=100' 5000 256
```

Sample output (virtual threads on):

```text
warmup: 1000 requests, concurrency 256 -> http://localhost:8080/route?symbol=AAPL&quantity=100
measure: 5000 requests, concurrency 256

requests=5000  wall=4521ms  throughput=1106 req/s
  p50    =   4.83 ms
  p99    =   8.21 ms
  p99.9  =  14.50 ms
  max    =  21.93 ms
```

Now flip `spring.threads.virtual.enabled=false`, restart, run the same load. The default Tomcat pool is 200 threads; at concurrency=256 we're queueing behind a busy slot for every excess request. Wall time triples, p99 climbs into the hundreds of milliseconds. The single property change is what shifts the shape.

Full source at [`cookbook/guides/java/subms-spring-boot-virtual-threads`](https://github.com/stochbook/cookbook/tree/main/guides/java/subms-spring-boot-virtual-threads).
