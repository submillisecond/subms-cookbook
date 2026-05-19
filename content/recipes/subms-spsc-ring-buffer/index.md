---
title: SPSC ring buffer
summary: Wait-free single-producer single-consumer ring. Padded counters and opposite-index caching keep enqueue and dequeue sub-microsecond at any contention you can give a 2-core link.
type: recipe
category: concurrency
repoPath: recipes/subms-spsc-ring-buffer
order: 7
difficulty: 3
loc: 250
languages: [rust, java]
topics:
  - concurrent-queues
prereqs:
  - "Atomic operations and memory ordering (acquire / release)"
  - "False sharing and cache line layout"
  - "Power-of-two index masking"
tags:
  - concurrency
  - concurrent-queues
  - lock-free
  - low-latency
perf:
  - { label: "enqueue p99",  value: "< 1 us", note: "100k op workload, 1024-slot buffer, sibling cores" }
  - { label: "dequeue p99",  value: "< 1 us", note: "same workload" }
  - { label: "padding",      value: "128 B",  note: "front + back guards on each counter; covers x86 and Apple Silicon prefetch line pairs" }
references:
  - { title: "JCTools SpscArrayQueue",  url: "https://github.com/JCTools/JCTools", note: "reference Java implementation we validate against" }
  - { title: "Agrona OneToOneConcurrentArrayQueue", url: "https://github.com/real-logic/agrona", note: "Aeron's SPSC; same shape" }
  - { title: "rtrb",                    url: "https://crates.io/crates/rtrb",      note: "reference Rust implementation" }
---

A bounded queue with one writer and one reader. Both threads run wait-free: at most one atomic load and one atomic store per op. Sized to a power of two, indexed by bitmask. The head and tail counters sit in padded cells so the producer's writes never invalidate the consumer's read line; each side caches the opposite index and only re-reads through the atomic when its own cache says "full" or "empty".

## The traps this recipe addresses

- **False sharing between head and tail.** Two `volatile long`s in the same cache line collapse throughput because every producer write invalidates the consumer's read of the head. The cookbook benchmarks the 10x gap with and without padding.
- **Opposite-index caching.** A naive impl re-reads the consumer head on every push, paying for a cache line bounce. Caching it locally and only refreshing on under-run is a 3x throughput win.
- **Memory ordering.** Release on publish; acquire on observe; relaxed on own-side reads. Get this wrong and the consumer reads garbage; get it too strong and you pay for fences you don't need.
- **Index wrap.** Use unbounded counters (`u64` / `long`) and mask on indexing into the slot array. Distinguishing full vs empty with modulo-capacity counters is the classic bug.

## Quality bar

**Reference implementation:** JCTools `SpscArrayQueue`, Agrona `OneToOneConcurrentArrayQueue`, `rtrb`. The Rust integration test cross-checks ordering against expected values under 1 M ops over two threads.

**Sub-ms claim under:** enqueue/dequeue p99 under 1 ms across 100k operations, capacity 1024, sibling-core producer/consumer.

**Not claimed:** multi-producer or multi-consumer; throughput on logical-core (HT) sibling pairs; anything past 1 M ops on a sustained basis where TLB pressure starts to dominate.
