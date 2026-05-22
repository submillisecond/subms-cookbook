---
title: Block cache
summary: Clock-sweep block cache with constant-time eviction. Approximates LRU; pays one hash probe + one slot bump per get.
type: recipe
category: memory
repoPath: recipes/subms-block-cache
order: 15
level: L200
loc: 210
languages: [rust, java]
prereqs:
  - "Hash maps (any backing store)"
  - "Clock-sweep eviction (second-chance LRU)"
tags:
  - memory
  - caching
  - low-latency
perf:
  - { label: "get p99 (hit)", value: "< 100 ns" }
  - { label: "put p99",       value: "< 200 ns", note: "amortised; sweep walks at most capacity slots" }
references:
  - { title: "Caffeine (Java)", url: "https://github.com/ben-manes/caffeine", note: "W-TinyLFU; the modern production reference" }
  - { title: "lru (Rust)",      url: "https://crates.io/crates/lru" }
---

Fixed-capacity cache. Each slot has a referenced bit. On full-capacity insert, the clock hand walks the ring: a set bit becomes clear; a clear bit gets evicted. Reads set the bit. Constant time per op; near-LRU eviction quality with one bit per slot instead of two linked-list pointers.

Pairs with the segment reader and merge iterator under [[log-and-segment-primitives]].

## Quality bar

**Reference impl:** Caffeine (Java) for eviction-order cross-check; `lru` (Rust) for the LRU baseline.

**Sub-ms claim under:** get p99 < 100 ns on hit; insert p99 < 200 ns (sweep walks at most one full lap before evicting).

**Not claimed:** thrashing workloads where working-set exceeds 2x cache size (the hit rate collapses; perf claim assumes > 80% hit rate); concurrent access (single-threaded).
