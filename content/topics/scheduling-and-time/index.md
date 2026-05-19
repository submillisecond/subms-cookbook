---
title: Scheduling and time
summary: Time-driven gates. Hierarchical timer wheels and lock-free token buckets that stay sub-microsecond at any contention.
type: topic
order: 4
recipes:
  - recipes/subms-timer-wheel
  - recipes/subms-rate-limiter
tags:
  - concurrency
  - scheduling
---

Every long-lived system eventually needs two things: a way to fire callbacks after a delay, and a way to throttle a stream of work to a target rate. The naive implementations of each are obvious traps - a `PriorityQueue` of timers becomes the system bottleneck under load, and a token bucket guarded by a mutex serialises every request through one contended lock.

The two recipes here are the canonical fixes:

- **Hierarchical timer wheel** trades the priority-queue's `O(log n)` for `O(1)` schedule and cancel. A wheel of buckets advances one tick at a time; longer timers cascade through multiple levels (Varghese and Lauck, 1987). Netty, Kafka, and the Linux kernel all ride on a wheel; Netty's is a single-level approximation that degrades for long-range timers - the hierarchical version doesn't.
- **Lock-free token bucket** packs `(tokens_x256, last_refill_ns)` into a single atomic word and CAS-loops the refill. No mutex; no doublespend; sub-microsecond `tryAcquire` even under contention. Bucket4j and `governor` ship variants of this; the packed-atomic CAS pattern reappears in HdrHistogram's recording path and the arena allocator's bump pointer.

Both recipes wear the same dial: tick granularity for the wheel, refill rate for the bucket. Pick a granularity that matches your domain, not your wall clock.
