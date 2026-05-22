---
lang: rust
---

## Quickstart

```toml
[dependencies]
subms-spsc-ring-buffer = "0.3"
```

```rust
use std::thread;
use subms_spsc_ring_buffer::SpscRingBuffer;

let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(1024);

let producer = thread::spawn(move || {
    for i in 0..10 { while tx.try_push(i).is_err() {} }
});

let mut seen = vec![];
while seen.len() < 10 {
    if let Some(v) = rx.try_pop() { seen.push(v); }
}
producer.join().unwrap();
assert_eq!(seen, (0..10).collect::<Vec<_>>());
```

For the perf-harness adapter, enable the `harness` feature - that pulls in `subms` and the `SubMsRecipe` impl.

### Step 1 - padded counters

```rust
#[repr(align(128))]
struct Padded(AtomicUsize);
```

128 bytes is the right padding for Apple Silicon and recent x86. Some prefetchers operate on pairs of cache lines, so 64-byte padding is occasionally not enough. Wasting ~96 bytes per counter is negligible against the false-sharing tax.

### Step 2 - opposite-index caching

Both `Producer::try_push` and `Consumer::try_pop` cache the opposite side's index locally. They re-read through the atomic only when their cache says full / empty:

```rust
let tail = self.inner.tail.0.load(Ordering::Relaxed);
if tail.wrapping_sub(self.cached_head) == self.inner.capacity {
    self.cached_head = self.inner.head.0.load(Ordering::Acquire);
    if tail.wrapping_sub(self.cached_head) == self.inner.capacity {
        return Err(value);
    }
}
```

The cached value is allowed to be stale; staleness only causes a *spurious "full"*, which falls through to the atomic re-read.

### Step 3 - memory ordering

- Publish (writer-side store of own counter): `Ordering::Release`.
- Observe (reader-side load of opposite counter): `Ordering::Acquire`.
- Own-side cached reads: `Ordering::Relaxed`.

That's the minimum that gives the consumer a happens-before edge to the value the producer wrote.

### Step 4 - Producer / Consumer split

The library returns a `(Producer<T>, Consumer<T>)` pair. The two handles share the `Arc<Inner<T>>` but can be moved to different threads independently. There is no way to call `try_push` from the consumer side at the type level - the SPSC invariant is encoded into the API, not relied on as a comment.
