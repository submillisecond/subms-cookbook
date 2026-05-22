---
lang: java
---

## Quickstart

```xml
<dependency>
    <groupId>com.submillisecond.recipes</groupId>
    <artifactId>subms-lsm-tree</artifactId>
    <version>0.3.0</version>
</dependency>
```

```java
import com.submillisecond.recipes.lsm.BloomMode;
import com.submillisecond.recipes.lsm.LsmTree;
import java.nio.file.Files;

Path dir = Files.createTempDirectory("lsm-quickstart");
try (LsmTree lsm = new LsmTree(dir, 16_000, BloomMode.ON)) {
    lsm.put("alice", "42");
    assert "42".equals(lsm.get("alice"));
}
```

`subms` + `subms-bloom-filter` pull in transitively. `LsmTreeRecipe` is available out of the box if you want to register the workload with the cookbook harness.

### Step 1 - the memtable

A `TreeMap` keyed on `String`, values as `byte[]`. `null` is a tombstone - *present* in the map, marking the key as deleted, so a later flush carries the deletion onto disk.

```java
final class Memtable {
    private final TreeMap<String, byte[]> entries = new TreeMap<>();
    private int approxSizeBytes = 0;

    void put(String key, byte[] value) {
        byte[] prev = entries.put(key, value);
        if (prev == null) {
            approxSizeBytes += key.length() + valueCost(value);
        } else {
            approxSizeBytes += valueCost(value) - valueCost(prev);
        }
    }

    Optional<Lookup> get(String key) {
        if (!entries.containsKey(key)) return Optional.empty();
        return Optional.of(new Lookup(entries.get(key)));
    }

    record Lookup(byte[] value) {
        boolean isTombstone() { return value == null; }
    }
}
```

The three-valued return is shaped through `Optional<Lookup>` instead of `Optional<Optional<...>>`: `Optional.empty()` is "not in this layer", a present `Lookup` with `value == null` is a tombstone, a present `Lookup` with bytes is a hit. Same semantics as Rust's `Option<Option<...>>`.

### Step 2 - the SSTable, with bloom trailer

On `open()`, read the whole file via `Files.readAllBytes` and parse the bloom out of the trailer. After this, a get never touches the filesystem.

```java
import com.submillisecond.recipes.bloom.BloomFilter;

final class SSTable {
    private static final int  MAGIC          = 0x4C534D54;     // "LSMT"
    private static final int  FOOTER_BYTES   = 8 + 4;
    private static final byte FLAG_TOMBSTONE = 0x01;

    private final byte[] buf;
    private final int    recordsEnd;
    private final BloomFilter bloom;

    static SSTable open(Path path) throws IOException {
        byte[] buf = Files.readAllBytes(path);
        int magic = readIntBE(buf, buf.length - 4);
        if (magic != MAGIC) throw new IOException("bad SSTable magic");
        long recordsEndLong = readLongBE(buf, buf.length - FOOTER_BYTES);
        int recordsEnd = (int) recordsEndLong;
        BloomFilter bloom = BloomFilter.parse(buf, recordsEnd, buf.length - FOOTER_BYTES - recordsEnd);
        return new SSTable(buf, recordsEnd, bloom);
    }
}
```

The footer (last 12 bytes) anchors navigation: it tells us where the records end and the bloom section begins, without scanning the file. `BloomFilter.parse(byte[], int, int)` accepts a slice so we hand it a zero-copy view of the parent buffer.

### Step 3 - the read path, bloom-checked

The bloom probe goes first. A negative answer is final; a positive falls through to a linear scan of the in-memory buffer.

```java
Optional<Hit> get(String key, boolean checkBloom) {
    if (checkBloom && !bloom.mightContain(key)) return Optional.empty();

    byte[] keyBytes = key.getBytes(StandardCharsets.UTF_8);
    int p = 0;
    while (p < recordsEnd) {
        int keyLen = readIntBE(buf, p); p += 4;
        int cmp = compareKey(buf, p, keyLen, keyBytes);
        p += keyLen;
        byte flag = buf[p]; p += 1;
        int valueLen = readIntBE(buf, p); p += 4;
        if (cmp == 0) {
            if (flag == FLAG_TOMBSTONE) return Optional.of(new Hit(null));
            byte[] value = new byte[valueLen];
            System.arraycopy(buf, p, value, 0, valueLen);
            return Optional.of(new Hit(value));
        }
        if (cmp > 0) return Optional.empty();   // sorted file: passed it
        p += valueLen;
    }
    return Optional.empty();
}

record Hit(byte[] value) {
    boolean isTombstone() { return value == null; }
}
```

`checkBloom` is the `BloomMode` knob plumbed down from the LSM coordinator. With it `true`, miss latency is dominated by seven hash probes per SSTable. With it `false`, you pay a full scan of every SSTable in the walk - that's the demonstrable case for the optimisation.

### Step 4 - the LSM coordinator

The top-level read walks memtable then SSTables newest-first; the bloom mode flows through to each `SSTable.get`.

```java
public enum BloomMode { ON, OFF }

public final class LsmTree implements AutoCloseable {
    private final BloomMode bloomMode;
    private final Memtable memtable = new Memtable();
    private final List<SSTable> sstables = new ArrayList<>();

    public Optional<String> get(String key) throws IOException {
        Optional<Memtable.Lookup> mem = memtable.get(key);
        if (mem.isPresent()) {
            return mem.get().isTombstone()
                ? Optional.empty()
                : Optional.of(new String(mem.get().value(), StandardCharsets.UTF_8));
        }
        boolean checkBloom = bloomMode == BloomMode.ON;
        for (int i = sstables.size() - 1; i >= 0; i--) {      // newest first
            Optional<SSTable.Hit> hit = sstables.get(i).get(key, checkBloom);
            if (hit.isPresent()) {
                return hit.get().isTombstone()
                    ? Optional.empty()
                    : Optional.of(new String(hit.get().value(), StandardCharsets.UTF_8));
            }
        }
        return Optional.empty();
    }
}
```

A tombstone returned from any layer collapses to `Optional.empty()` for the caller - they see "absent" without knowing if it was never written or actively deleted.

### Step 5 - the perf test

The cookbook's perf test runs both bloom modes back-to-back, prints percentiles via the shared `SubMsBench.printSummary` formatter, and asserts p99 < 1 ms on the on pass:

```text
$ java -cp target/classes:<deps> com.submillisecond.recipes.lsm.SubmillisecondBench

entries=50000  flush_threshold_bytes=16000  warmup=5000

bloom = on
  stage            p50        p99      p99.9        max       mean
  put            300ns      1.2us    153.9us     3.90ms      1.8us
  get_hit        8.2us     29.5us     85.0us     3.77ms      9.0us
  get_miss       2.4us     29.8us     86.3us    189.1us      6.0us

bloom = off
  stage            p50        p99      p99.9        max       mean
  put            300ns      1.0us     26.6us     2.23ms      1.3us
  get_hit       13.4us    955.3us     1.18ms     5.94ms    132.4us
  get_miss     793.5us     1.50ms     2.97ms     8.10ms    872.7us

OK (BloomMode.ON - all p99 < 1ms)
```

Same shape as the Rust numbers: bloom on stays sub-1 ms at p99 across the board; bloom off blows past 1 ms on miss p99. Java is ~2x slower per percentile than Rust on this workload (Java get_hit p99 ~30us vs Rust ~15us; Java get_miss p99 ~30us vs Rust ~18us) - mostly UTF-8 decoding cost and per-call boxing, not anything architectural. The `max` spikes are honest steady-state JVM noise: JIT compilation events that survive the warmup window, plus young-gen GC. The bench's contract is p99, not max.

Full source at [`cookbook/recipes/subms-lsm-tree/java`](https://github.com/submillisecond/subms-cookbook/tree/main/recipes/subms-lsm-tree/java).
