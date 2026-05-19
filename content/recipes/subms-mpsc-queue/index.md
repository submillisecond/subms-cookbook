---
title: MPSC queue
summary: Vyukov's multi-producer single-consumer linked queue. Producers swap the head; consumer walks `next` pointers. The dangling-tail window is the whole story.
type: recipe
category: concurrency
repoPath: recipes/subms-mpsc-queue
order: 8
difficulty: 3
loc: 280
languages: [rust, java]
topics:
  - concurrent-queues
prereqs:
  - "Atomic CAS and swap"
  - "Linked-list publication patterns"
  - "Acquire / release memory ordering"
tags:
  - concurrency
  - concurrent-queues
  - lock-free
  - low-latency
perf:
  - { label: "offer p99",  value: "< 1 us", note: "4-producer contention, 40k op workload" }
  - { label: "poll p99",   value: "< 1 us", note: "single consumer drains under same load" }
references:
  - { title: "JCTools MpscLinkedQueue", url: "https://github.com/JCTools/JCTools", note: "reference Java port of Vyukov's design" }
  - { title: "1024cores: MPSC algorithm", url: "https://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue", note: "Dmitry Vyukov's original writeup" }
---

A linked queue where any thread can `push` and exactly one thread can `tryPoll`. Push is wait-free once the node is allocated: swap the head, then release-store the link from the previous node. The consumer walks from a sentinel stub through `next` pointers and frees consumed nodes as it goes.

The load-bearing detail is the **dangling-tail window**: between the producer's swap-of-head and the link write, the consumer can observe `tail.next == null` while another thread is mid-publish. Returning "empty" in that window loses items. The Rust impl distinguishes via `PopResult::Inconsistent`; the Java impl returns `null` from `tryPoll()` and offers `isInconsistent()` so the caller can spin instead of giving up.

## Quality bar

**Reference impl:** JCTools `MpscLinkedQueue` (Java; direct port of Vyukov), crossbeam-queue `SegQueue` (Rust; Vyukov-attributed).

**Sub-ms claim under:** offer p99 < 1 ms at 4-producer contention, 40k ops; poll p99 < 1 ms.

**Not claimed:** unbounded memory growth under sustained backlog; > 64 producer contention (CAS storms dominate).
