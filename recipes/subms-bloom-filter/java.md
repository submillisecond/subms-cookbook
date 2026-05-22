---
lang: java
---

## Quickstart

```xml
<dependency>
    <groupId>com.submillisecond.recipes</groupId>
    <artifactId>subms-bloom-filter</artifactId>
    <version>0.3.0</version>
</dependency>
```

```java
import com.submillisecond.recipes.bloom.BloomFilter;

BloomFilter bf = new BloomFilter(10_000);
bf.add("alice");
assert bf.mightContain("alice");
assert !bf.mightContain("bob");
```

The recipe pulls in `com.submillisecond:subms` transitively, so the `SubMsRecipe` interface and `SubMsBench` helpers are available without any extra dep if you want to register this recipe with the cookbook harness.

### Step 1 - the class

`long[]` for the bit array (one cache-friendly word per 64 bits), a fixed `k`, and a deterministic non-cryptographic hash.

```java
public final class BloomFilter {

    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME  = 0x100000001b3L;

    private final int bitCount;
    private final int k;
    private final long[] bits;

    public BloomFilter(int expectedEntries) {
        this.bitCount = Math.max(64, expectedEntries * 10);
        this.k = 7;
        this.bits = new long[(bitCount + 63) >>> 6];
    }

    private static long fnv1a64(String key) {
        long h = FNV_OFFSET;
        for (byte b : key.getBytes(StandardCharsets.UTF_8)) {
            h ^= b & 0xffL;
            h *= FNV_PRIME;
        }
        return h;
    }
}
```

`Math.max(64, ...)` is the 64-bit floor so `bitCount = 0` is impossible. `(bitCount + 63) >>> 6` is the same as `ceil(bitCount / 64.0)`, expressed in integer ops.

### Step 2 - add & mightContain, double-hashed

One FNV-1a call per operation, split into two halves, seven probes.

```java
public void add(String key) {
    long h = fnv1a64(key);
    int h1 = (int) h;
    int h2 = ((int) (h >>> 32)) | 1;     // force odd so we never degenerate
    for (int i = 0; i < k; i++) {
        int idx = Math.floorMod(h1 + i * h2, bitCount);
        bits[idx >>> 6] |= 1L << (idx & 63);
    }
}

public boolean mightContain(String key) {
    long h = fnv1a64(key);
    int h1 = (int) h;
    int h2 = ((int) (h >>> 32)) | 1;
    for (int i = 0; i < k; i++) {
        int idx = Math.floorMod(h1 + i * h2, bitCount);
        if ((bits[idx >>> 6] & (1L << (idx & 63))) == 0) return false;
    }
    return true;
}
```

`Math.floorMod` is the variant that handles negative dividends correctly - `h1 + i * h2` can overflow into negative `int` range, and we want the modular reduction, not Java's truncating `%`. `idx >>> 6` is `idx / 64`, `idx & 63` is `idx % 64` - both single-cycle ops the JIT will produce anyway, but explicit is faster to read.

### Step 3 - serialisation

Fixed wire format so a filter written by one process (or one language) is readable by another.

```java
public void writeTo(DataOutputStream out) throws IOException {
    out.writeInt(bitCount);
    out.writeInt(k);
    out.writeInt(bits.length);
    for (long w : bits) out.writeLong(w);
}

public static BloomFilter parse(byte[] buf, int off, int len) throws IOException {
    try (DataInputStream in = new DataInputStream(new ByteArrayInputStream(buf, off, len))) {
        int bitCount = in.readInt();
        int k        = in.readInt();
        int words    = in.readInt();
        long[] bits  = new long[words];
        for (int i = 0; i < words; i++) bits[i] = in.readLong();
        return new BloomFilter(bitCount, k, bits);
    }
}
```

`parse(byte[], int, int)` accepts a slice so the SSTable trailer can hand it a view into the parent file's byte buffer without copying.

### Step 4 - verify the FPR

The cookbook's correctness test pins it at ~1%:

```java
private static void falsePositiveRateIsRoughly1Percent() {
    int n = 10_000;
    BloomFilter bf = new BloomFilter(n);
    for (int i = 0; i < n; i++) bf.add("present" + i);

    int probes = 100_000;
    int falsePositives = 0;
    for (int i = 0; i < probes; i++) {
        if (bf.mightContain("absent" + i)) falsePositives++;
    }
    double fpr = (double) falsePositives / probes;
    check(fpr < 0.05, String.format("fpr %.4f too high", fpr));
}
```

5% is generous headroom; a typical run sits around 1%. The full source is at [`cookbook/recipes/subms-bloom-filter/java`](https://github.com/submillisecond/subms-cookbook/tree/main/recipes/subms-bloom-filter/java) - runnable with plain `javac` and `java`, no Maven needed.
