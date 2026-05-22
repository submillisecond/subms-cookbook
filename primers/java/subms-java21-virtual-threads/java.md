---
lang: java
---

### Step 1 - spawn a virtual thread

The whole virtual-threads API change in one line. `Thread.ofVirtual()` returns a builder; `.start(r)` schedules `r` on the JVM's carrier-thread pool but the runnable is *not* pinned to an OS thread for its lifetime.

```java
Thread t = Thread.ofVirtual().start(() -> {
    LockSupport.parkNanos(Duration.ofMillis(10).toNanos());
    // ... runs on whichever carrier the scheduler picks ...
});
t.join();
```

100,000 of those in a row land in well under a second of wall time on any modern laptop. `VirtualThreadDemo` is the canonical micro-proof.

### Step 2 - one executor per workload

Use `Executors.newVirtualThreadPerTaskExecutor()` for IO-shaped concurrency. It is genuinely "one new thread per submitted task." No pool size to tune, no rejection policy to think about.

```java
try (ExecutorService exec = Executors.newVirtualThreadPerTaskExecutor()) {
    List<Future<Void>> futures = requests.stream()
            .map(r -> exec.submit(() -> handle(r)))
            .toList();
    for (Future<Void> f : futures) f.get();
}
```

For CPU-bound work, the old `ForkJoinPool.commonPool()` or a sized `ThreadPoolExecutor` is still the right answer - virtual threads are about cheap parking, not cheap computation.

### Step 3 - the bench

`ConcurrencyBench` submits 10,000 independent tasks; each parks for 20 ms of simulated IO. The same workload runs twice, through two executors:

```java
Result vthread  = runOnce("vthread",
        Executors::newVirtualThreadPerTaskExecutor, TASKS, true);
Result platform = runOnce("platform pool=64",
        () -> Executors.newFixedThreadPool(64, platformThreadFactory()),
        TASKS, true);

double speedup = (double) platform.wallMillis / vthread.wallMillis;
if (speedup < 10) {
    throw new AssertionError("expected >= 10x speedup; got " + speedup);
}
```

Typical output:

```text
vthread                tasks=10000  park=20000us  wall=69ms    p50=39.76ms  p99=52.56ms  p999=52.71ms  max=52.72ms
platform pool=64       tasks=10000  park=20000us  wall=4952ms  p50=2516.36ms p99=4898.73ms p999=4946.90ms max=4946.96ms

Speedup (platform / vthread): 71.8x wall
```

The platform pool's p99 task latency is essentially the wall time - the median task waits half the run for its turn at one of 64 slots. The vthread executor doesn't gate on carrier count for parked tasks, so the wall stays close to a small multiple of the park time itself.

Two specific design choices in the bench worth calling out:

- **Flat parallelism, not fan-out.** A previous version had each "request" task submit four "backend call" sub-tasks to the same fixed pool. With a 64-thread pool that's the canonical thread-pool starvation deadlock: the 64 parents pin every slot, the children queue waiting for a slot that will never free. Flat submission sidesteps that and is also a cleaner comparison.
- **Park time clear of OS timer slop.** On Windows the default kernel timer resolution rounds short `LockSupport.parkNanos` calls up to ~15.6 ms. 20 ms is well above that so the measurement reflects scheduling, not the timer.

### Step 4 - record patterns + switch patterns

A sealed `OrderEvent` ADT with record components:

```java
sealed interface OrderEvent permits New, Fill, Cancel, Reject {}
record New(long id, String symbol, long qty, long pxTicks)  implements OrderEvent {}
record Fill(long id, long qty, long pxTicks)                 implements OrderEvent {}
record Cancel(long id, String reason)                        implements OrderEvent {}
record Reject(long id, String reason)                        implements OrderEvent {}
```

The walk-the-tape function is a single exhaustive switch. Each branch destructures the record's fields directly - no accessor calls, no `instanceof`, no casts:

```java
return switch (e) {
    case New(var id, var symbol, var qty, var pxTicks)
            -> "new  #" + id + "  " + qty + " " + symbol + " @ " + pxTicks;
    case Fill(var id, var qty, var pxTicks)
            -> "fill #" + id + "  " + qty + " @ " + pxTicks;
    case Cancel(var id, var reason)
            -> "cxl  #" + id + "  (" + reason + ")";
    case Reject(var id, var reason)
            -> "rej  #" + id + "  (" + reason + ")";
};
```

Guards split a single type into multiple branches by predicate, no default needed because the sealed permits list is exhaustive:

```java
return switch (e) {
    case Fill f when f.qty() >= 1_000  -> "block-trade";
    case Fill f                         -> "ordinary-fill";
    case Cancel c when c.reason().contains("timeout") -> "stale-cancel";
    case Cancel c                       -> "user-cancel";
    case New n                          -> "new-order";
    case Reject r                       -> "reject";
};
```

Compilation fails if you add a fifth permitted subtype without a matching case, which is the whole point.

### Step 5 - sequenced collections

`reversed()` returns a view, not a copy:

```java
SequencedCollection<OrderEvent> latestFirst = tape.reversed();
latestFirst.forEach(e -> System.out.println("  " + describe(e)));
```

And `firstEntry()` / `lastEntry()` on `LinkedHashMap` end a small genre of one-off iterator code:

```java
LinkedHashMap<Long, String> byId = new LinkedHashMap<>();
// ... populate ...
System.out.println("first: " + byId.firstEntry());
System.out.println("last:  " + byId.lastEntry());
```

### Run it

```sh
cd cookbook/primers/java/subms-java21-virtual-threads
mvn -q package
java -cp target/classes com.submillisecond.primers.java21.VirtualThreadDemo
java -cp target/classes com.submillisecond.primers.java21.PatternMatchingDemo
java -cp target/classes com.submillisecond.primers.java21.ConcurrencyBench
java -cp target/classes com.submillisecond.primers.java21.Tests
```

Full source at [`cookbook/primers/java/subms-java21-virtual-threads`](https://github.com/submillisecond/subms-cookbook/tree/main/primers/java/subms-java21-virtual-threads).
