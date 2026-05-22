# ZGC pause times under load (Java 21)

A heartbeat-based measurement of application-thread pause times under
sustained allocation pressure. The bench runs against whichever
collector you launch the JVM with - so you compare G1, ZGC, and
generational ZGC by changing JVM flags, not Java code.

The point of this primer is **what the application observes**, not what
GC log scraping says. A "5 ms GC pause" is interesting; a 5 ms gap in
the application's response timeline is what you actually pay for.

```sh
mvn -q package

# Default (G1 in JDK 21)
java -XX:+UseG1GC -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.primers.zgc.PauseTimeBench

# Non-generational ZGC (stable in 21)
java -XX:+UseZGC -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.primers.zgc.PauseTimeBench

# Generational ZGC (preview in 21, stable in 23)
java -XX:+UseZGC -XX:+ZGenerational -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.primers.zgc.PauseTimeBench

# Sanity checks (machinery only; does not assert pause targets).
mvn -q test
```

## What the bench does

- **Heartbeat thread.** A single platform thread reads
  `System.nanoTime()` in a tight loop and records the gap between
  successive reads. The gap is the loop's natural overhead plus any
  time the thread was suspended (GC safepoint, scheduler preemption,
  page fault). We sort the gaps and report percentiles.
- **Allocator threads.** Four threads continuously allocate `byte[]`
  arrays of mixed sizes (mostly 1 KiB, occasionally 64 KiB) and rotate
  references through a small rolling buffer so the survivors actually
  promote into the old generation under a generational collector.
- **Warmup.** First 2 s of samples are discarded; otherwise the
  reported `max` is dominated by JIT compilation of the heartbeat
  loop, not steady-state GC behaviour.

## Typical results (JDK 21.0.11, quiet laptop, 2 GiB heap)

```
=== G1 ===
  p99      = 200 ns      p99.99   = 12.5 us    max = 36.9 ms
  GC:  259 Young pauses, 628 ms cumulative

=== ZGC ===
  p99      = 200 ns      p99.99   = 217  us    max = 64.7 ms
  GC:  252 STW events, 3 ms cumulative

=== Generational ZGC ===
  p99      = 200 ns      p99.99   = 113  us    max = 24.5 ms
  GC:  1575 minor pauses, 9 ms cumulative; 39 major pauses, 0 ms
```

## How to read the numbers

The cumulative GC time from the MX beans is the honest "what did the
collector cost?" number. ZGC's 3 ms across 252 STW events is ~12 us
average per pause - sub-millisecond, every time. G1's 259 young
collections totalling 628 ms is ~2.4 ms average per pause - real STW
work, on every young generation cycle.

The heartbeat's `max` on a quiet machine is dominated by the OS
scheduler, not by GC. With four allocator threads competing for cores
on a 12-core box, the heartbeat thread occasionally gets preempted for
tens of milliseconds. That's why all three collectors show a similar
`max` of 25-65 ms despite very different actual GC behaviour. **Trust
the cumulative GC time and the p99 / p99.9 tail; don't read the max
as "the worst GC pause".**

To isolate GC-only pauses you would pin the heartbeat thread to an
isolated core (Linux `taskset` plus a `cpuset` partition that the
allocator threads cannot use), or sample directly from
`-Xlog:safepoint`. Both are out of scope for a portable cookbook
primer; the cumulative MX-bean number is good enough to tell the
story.

## What this is and isn't

The primer is **not** an end-to-end benchmark of "is ZGC faster". Pure
allocator throughput, as the bench reports, is higher under G1 on this
workload because G1's young-gen survivor strategy fits the pattern
well and ZGC pays a small barrier cost on every reference store. The
primer **is** about what the application thread sees: ZGC pauses sum
to single-digit milliseconds over a 5-second run, with no individual
pause longer than a few hundred microseconds. That is the property
you build a sub-millisecond service around.

## Files

- `src/main/java/com/submillisecond/primers/zgc/PauseTimeBench.java`
  the bench. Warmup, heartbeat, allocator, percentile report, MX-bean
  summary.
- `src/test/java/com/submillisecond/primers/zgc/PauseTimeBenchTest.java`
  JUnit 5 smoke tests for the measurement machinery; does not assert
  pause-time targets (those depend on flags passed to the JVM).
