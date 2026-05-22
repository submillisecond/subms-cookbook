---
lang: java
---

## Quickstart

```xml
<dependency>
    <groupId>com.submillisecond.recipes</groupId>
    <artifactId>subms-spsc-ring-buffer</artifactId>
    <version>0.3.0</version>
</dependency>
```

```java
SpscRingBuffer<Integer> q = new SpscRingBuffer<>(1024);
SpscRingBuffer<Integer>.Producer p = q.producer();
SpscRingBuffer<Integer>.Consumer c = q.consumer();

new Thread(() -> {
    for (int i = 0; i < 10; i++) while (!p.tryPush(i)) {}
}).start();

int got = 0;
while (got < 10) {
    Integer v = c.tryPop();
    if (v != null) { System.out.println(v); got++; }
}
```

### Step 1 - padded counter cells

```java
private static final class PaddedLong {
    long p1, p2, p3, p4, p5, p6, p7;
    volatile long value;
    long q1, q2, q3, q4, q5, q6, q7;
}
```

Seven `long`s of pre-padding and seven of post-padding bracket the `value` field, so the JVM cannot pack the counter into the same cache line as something else in the same object header. 128 bytes total guard.

### Step 2 - VarHandle, not AtomicLong

```java
VH = MethodHandles.lookup().findVarHandle(PaddedLong.class, "value", long.class);

VH.setRelease(tail, t + 1);          // publish
long h = (long) VH.getAcquire(head); // observe
long t = (long) VH.getOpaque(tail);  // own-side cached read
```

`AtomicLong` would force a separate object allocation and dodge the inline padding, plus it carries CAS semantics we never need. `VarHandle` lets us name the exact ordering on each access - opaque (relaxed-equivalent), acquire, release - and works against the inline `long` field directly.

### Step 3 - opposite-index caching

Same pattern as the Rust side:

```java
public boolean tryPush(T value) {
    long t = (long) PaddedLong.VH.getOpaque(tail);
    if (t - cachedHead == capacity) {
        cachedHead = (long) PaddedLong.VH.getAcquire(head);
        if (t - cachedHead == capacity) return false;
    }
    buf[(int) (t & mask)] = value;
    PaddedLong.VH.setRelease(tail, t + 1);
    return true;
}
```

### Step 4 - releasing references on pop

The consumer sets the slot back to `null` after reading. The producer never observes that slot until `head` advances past it (acquire/release order makes that safe), but the reference clear lets the value become collectible immediately - otherwise the queue keeps a strong reference to every value that ever passed through.
