---
title: Timer wheel
summary: Single-level hashed timer wheel. O(1) schedule and cancel; tick fires every timer with rounds=0 in the current bucket.
type: recipe
category: concurrency
repoPath: recipes/subms-timer-wheel
order: 20
difficulty: 3
loc: 220
languages: [rust, java]
topics:
  - scheduling-and-time
prereqs:
  - "Modular index math"
  - "Rounds-counter vs cascade tradeoff (Netty vs Kafka)"
tags:
  - concurrency
  - scheduling
  - timer
perf:
  - { label: "schedule p99",  value: "< 100 ns" }
  - { label: "cancel p99",    value: "< 50 ns",  note: "lazy flag; cleanup amortised on tick" }
  - { label: "tick p99",      value: "< 1 ms",   note: "1024 slots; visits one bucket per tick" }
references:
  - { title: "Netty HashedWheelTimer", url: "https://netty.io/4.1/api/io/netty/util/HashedWheelTimer.html" }
  - { title: "Varghese and Lauck timer wheels", url: "https://www.cs.columbia.edu/~nahum/w6998/papers/sosp87-timing-wheels.pdf" }
---

A wheel of `N` slots (power of two). A scheduled timer at delay `d` ticks lives in slot `(hand + d) % N` with a rounds counter of `d / N`. Each tick advances the hand one slot; timers with rounds 0 fire; the rest decrement. Cancel sets a lazy flag; the sweep on the next visit to that bucket drops cancelled entries.

This is the Netty `HashedWheelTimer` shape - single level, rounds-counter rather than hierarchical cascade. Hierarchical would handle long-range timers without no-op revolutions; the recipe documents that as a tradeoff and shows the simpler form.

## Quality bar

**Reference impl:** Netty `HashedWheelTimer`. The original Varghese-Lauck paper describes the hierarchical variant; this recipe is the simpler single-level cousin.

**Sub-ms claim under:** schedule p99 < 100 ns; cancel p99 < 50 ns; tick p99 < 1 ms with 1024 slots and ~30k pending timers.

**Not claimed:** long-range timers (where `delay >> N` causes many no-op revolutions; switch to hierarchical for that); cross-thread schedule (single-threaded; use the [[concurrent-queues]] recipes as an inbox for the multi-thread variant).
