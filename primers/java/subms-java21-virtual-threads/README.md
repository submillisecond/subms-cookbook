# Java 21 virtual threads (and friends)

A walkthrough of the Java 21 features most relevant to a low-latency
trading codebase, all running against a realistic submillisecond fan-out
workload. Stable features only - no `--enable-preview`.

Features in scope:

- **Virtual threads** (JEP 444). The headline. One-OS-thread-per-task
  goes away; tasks park cheaply, so you can spin tens of thousands of
  concurrent IO-bound operations without a thread pool.
- **Record patterns** (JEP 440) and **pattern matching for switch**
  (JEP 441). Destructure straight into branches, no `instanceof` ladder,
  compiler-enforced exhaustiveness against a sealed hierarchy.
- **Sequenced collections** (JEP 431). `List.reversed()`,
  `LinkedHashMap.firstEntry()` / `lastEntry()` without writing the
  iteration yourself.

```sh
mvn -q package
mvn -q test                                                                       # JUnit 5: vthread + pattern matching tests

# 100,000 virtual threads, each parks 10ms; verify the JVM scales the way the JEP claims.
java -cp target/classes com.submillisecond.primers.java21.VirtualThreadDemo

# Order-event ADT through record + switch patterns and sequenced collections.
java -cp target/classes com.submillisecond.primers.java21.PatternMatchingDemo

# Head-to-head: virtual threads vs a fixed platform-thread pool on a
# flat-parallel blocking workload. Asserts the speedup is >= 10x wall time.
java -cp target/classes com.submillisecond.primers.java21.ConcurrencyBench
```

## What `ConcurrencyBench` measures

10,000 independent tasks, each parking 20 ms of simulated IO. Flat
parallelism (no fan-out) so the platform pool can't deadlock against
itself - the canonical thread-pool starvation trap. Run twice:

1. `Executors.newVirtualThreadPerTaskExecutor()` - one virtual thread
   per submitted task. No pool.
2. `Executors.newFixedThreadPool(64)` - a generously-sized platform
   pool by HFT standards.

The bench prints wall time and p50 / p99 / p999 / max per-task latency
for each, then asserts virtual threads beat the platform pool by at
least 10x wall time. Typical observed margin on a quiet laptop is
50-200x.

This is the canonical shape where virtual threads shine: lots of
concurrency, each task mostly parked, no CPU work to speak of. CPU-bound
workloads do not get faster - the JVM still needs cores to run them on.

## Files

- `src/main/java/com/submillisecond/primers/java21/VirtualThreadDemo.java`
  the smallest possible "is this thing virtual" - spawn 100k, each parks,
  assert they all complete.
- `src/main/java/com/submillisecond/primers/java21/ConcurrencyBench.java`
  head-to-head against a platform pool on a fan-out workload.
- `src/main/java/com/submillisecond/primers/java21/PatternMatchingDemo.java`
  sealed `OrderEvent` ADT, switch with record patterns and guards,
  `reversed()` and `firstEntry()` on sequenced collections.
- `src/test/java/com/submillisecond/primers/java21/VirtualThreadDemoTest.java`
  JUnit 5 checks for the virtual-thread API.
- `src/test/java/com/submillisecond/primers/java21/PatternMatchingDemoTest.java`
  JUnit 5 checks for every branch + guard of the pattern-matched switches.
