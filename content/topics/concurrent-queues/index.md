---
title: Concurrent queues
summary: Cross-thread links that do not block. The wait-free primitives every event loop and actor system ride on top of.
type: topic
order: 3
recipes:
  - recipes/subms-spsc-ring-buffer
  - recipes/subms-mpsc-queue
tags:
  - concurrency
  - concurrent-queues
---

Two threads pass values without locks. The producer writes; the consumer reads; both stay on different cores; nothing blocks; the link runs at memory-bandwidth speeds.

Get the memory ordering wrong and the consumer reads garbage. Get the cache layout wrong and the throughput collapses to 1/10th because the producer's writes invalidate the consumer's read line on every iteration (false sharing). Both recipes in this topic exist because the textbook implementations of "lock-free queue" routinely ship with one of those bugs.

- **SPSC ring buffer** is the single-producer single-consumer link. Wait-free in both directions. Cache-line padding between the head and tail counters. Opposite-index caching so the consumer only re-reads the producer's tail when it has caught up. Used in Aeron, the Disruptor, every audio engine.
- **MPSC linked queue** is Vyukov's classic. Producers CAS to enqueue at the tail; the consumer drains by following `next` pointers. The dangling-tail window between the CAS and the link write is the load-bearing detail; everyone reinvents the buggy version that ignores it.

Read SPSC first - it teaches the memory-ordering vocabulary. Read MPSC next - it teaches what happens when many producers contend.
