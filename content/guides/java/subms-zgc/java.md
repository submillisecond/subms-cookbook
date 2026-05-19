---
lang: java
---

### Step 1 - the heartbeat

The whole measurement rests on one tight loop. Successive reads of `System.nanoTime()` should be a few hundred nanoseconds apart on a modern CPU; anything larger is a suspension. The volatile sink defeats JIT dead-code elimination, otherwise the JVM correctly notices the loop produces no observable side effect and collapses it.

```java
long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(seconds);
long[] samples = new long[MAX_SAMPLES];
int n = 0;

long prev = System.nanoTime();
long sink = 0;
while (System.nanoTime() < deadline) {
    long now = System.nanoTime();
    long gap = now - prev;
    prev = now;
    if (n < samples.length) samples[n++] = gap;
    sink ^= now;
}
SINK = sink;            // volatile, escapes the loop
```

### Step 2 - sustained allocation pressure

Four allocator threads each run a tight loop allocating `byte[]` with a mix of sizes, holding references in a small rolling buffer so the survivors actually promote to the old generation under a generational collector. The `b[0]`/`b[size-1]` writes touch the array so the JIT doesn't optimise the allocation away.

```java
byte[][] survivors = new byte[256][];
int idx = 0;
while (!stop.get()) {
    int size = (rng.nextInt(8) == 0) ? LARGE_BYTES : SMALL_BYTES;
    byte[] b = new byte[size];
    b[0] = (byte) size;
    b[size - 1] = (byte) (size >>> 8);
    survivors[idx] = b;
    idx = (idx + 1) % survivors.length;
}
```

Allocator threads do **not** spin via `Thread.sleep` or `LockSupport.parkNanos`; that would let the GC catch up and the pressure would disappear. They run flat-out for the whole window.

### Step 3 - warmup and discard

Without a warmup window the reported `max` is dominated by JIT compilation of the heartbeat loop on the first hundred milliseconds of life. We run the same heartbeat for `WARMUP_SECONDS` and throw the samples away:

```java
long preGcCollections = totalGcCollections();
measureHeartbeat(stop, WARMUP_SECONDS);       // discarded
long[] gaps = measureHeartbeat(stop, RUN_SECONDS);
long collectionsDuringRun = totalGcCollections() - preGcCollections;

if (collectionsDuringRun == 0) {
    System.err.println("warning: no GC collections during the measurement window; allocator pressure too low");
}
```

If the measurement window saw zero GC cycles, the bench tells you - the comparison is meaningless without it.

### Step 4 - report

Sort the gap samples and print percentiles. The `formatNanos` helper switches units so the output stays readable across the full range.

```java
long[] sorted = gaps.clone();
Arrays.sort(sorted);
long p50  = sorted[(int) (sorted.length * 0.50)];
long p99  = sorted[(int) (sorted.length * 0.99)];
long p999 = sorted[(int) (sorted.length * 0.999)];
long max  = sorted[sorted.length - 1];
System.out.printf("p99 = %s   p99.9 = %s   max = %s%n",
        formatNanos(p99), formatNanos(p999), formatNanos(max));
```

And separately the MX-bean summary - the actual GC budget:

```java
for (GarbageCollectorMXBean gc : ManagementFactory.getGarbageCollectorMXBeans()) {
    System.out.printf("  %-30s collections=%d  cumulative_time=%dms%n",
            gc.getName(), gc.getCollectionCount(), gc.getCollectionTime());
}
```

`GarbageCollectorMXBean.getCollectionTime()` is the cumulative stop-the-world time in milliseconds across all collections of that bean, since JVM start. That is the cleanest "what did GC cost?" number you can get without scraping `-Xlog:gc`.

### Step 5 - flip the flag, run again

The bench has no GC-specific code path - the collector is selected by the JVM flags you launch with. That's the whole point: a single artefact, three runs, three numbers.

```sh
java -XX:+UseG1GC                    -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.guides.zgc.PauseTimeBench
java -XX:+UseZGC                     -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.guides.zgc.PauseTimeBench
java -XX:+UseZGC -XX:+ZGenerational  -Xms2g -Xmx2g \
     -cp target/classes com.submillisecond.guides.zgc.PauseTimeBench
```

The `-Xms2g -Xmx2g` matters: an elastic heap throws extra variables into the bench (sizing decisions during the run). Lock it down.

### What you'll see

Sample output for one ZGC run (JDK 21.0.11, 2 GiB heap):

```text
JVM:        OpenJDK 64-Bit Server VM 21.0.11+10-LTS
Collectors enabled (input args matter; default is G1):
  - ZGC Cycles
  - ZGC Pauses
Heap max:   2048 MiB
Cores:      12

warmup:    2199ms, 5,493,858 samples discarded

Heartbeat gaps (lower is better; max is the worst observed STW pause):
  samples = 12,301,230
  p50     =     100 ns
  p99     =     200 ns
  p99.9   =    3.30 us
  p99.99  =  217.10 us
  max     =  64.699 ms

Allocator: 100685.9 MiB total -> 20137.2 MiB/s sustained over 5s with 4 threads

GC summary:
  ZGC Cycles                     collections=84   cumulative_time=4968ms
  ZGC Pauses                     collections=252  cumulative_time=3ms
```

The 3 ms cumulative pause across 252 stop-the-world events is the headline. The 64 ms `max` heartbeat gap is *not* a GC pause - ZGC's worst pause on this run was an order of magnitude shorter than that. Read both numbers and you have the honest picture.

Full source at [`cookbook/guides/java/subms-zgc`](https://github.com/stochbook/cookbook/tree/main/guides/java/subms-zgc).
