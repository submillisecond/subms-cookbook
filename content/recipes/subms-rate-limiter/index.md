---
title: Rate limiter
summary: Lock-free GCRA rate limiter using a single-atomic CAS-loop. One `tryAcquire` is one load and one CAS, even at 8-way contention.
type: recipe
category: concurrency
repoPath: recipes/subms-rate-limiter
order: 9
difficulty: 3
loc: 220
languages: [rust, java]
topics:
  - scheduling-and-time
prereqs:
  - "Monotonic clocks and `Instant` / `System.nanoTime()`"
  - "CAS loops and ABA"
  - "Token bucket vs GCRA equivalence"
tags:
  - concurrency
  - scheduling
  - rate-limiting
  - low-latency
perf:
  - { label: "tryAcquire p99 (uncontended)", value: "< 100 ns" }
  - { label: "tryAcquire p99 (8-way)",       value: "< 300 ns" }
references:
  - { title: "Bucket4j", url: "https://github.com/bucket4j/bucket4j", note: "reference Java rate limiter; lock-free token bucket" }
  - { title: "governor (Rust)", url: "https://crates.io/crates/governor", note: "GCRA-based; same algorithm class" }
  - { title: "GCRA on Wikipedia", url: "https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm" }
---

State is a single `AtomicLong` (or `AtomicU64`) holding `tat_ns` - the theoretical arrival time of the next permit. `tryAcquire` reads `tat`, computes `max(now, tat) + period`, rejects if the new TAT would land more than `burst_ns` in the future, otherwise CAS-loops the update in. No mutex, no double-spend, no double-word state.

The Rust and Java implementations are line-for-line equivalent; the algorithm is the load-bearing thing, not the language.

## Quality bar

**Reference impl:** Bucket4j (Java, lock-free token bucket); `governor` (Rust, GCRA).

**Sub-ms claim under:** tryAcquire p99 < 100 ns uncontended; < 300 ns at 8-way contention.

**Not claimed:** distributed / cluster-scope rate limiting; fairness across waiters (this is non-blocking - it returns false, doesn't queue).
