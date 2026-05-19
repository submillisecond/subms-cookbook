---
title: Memory and observability
summary: Per-request lifetimes and how to measure them. The bump arena and the percentile sketch that ride alongside every hot path.
type: topic
order: 6
recipes:
  - recipes/subms-arena-allocator
  - recipes/subms-hdr-histogram
tags:
  - memory
  - observability
---

Two recipes you reach for the moment you stop tolerating allocator and tail-latency surprises:

- **Arena allocator** is a bump pointer with chunked growth and a `reset()` per request. Allocate a scratch String, parse JSON, write a response, reset; the entire request leaves zero garbage behind. `bumpalo` is the canonical Rust crate; the JDK 22 Foreign Memory API gives you the same thing for off-heap Java. The trap (called out in the recipe): bumpalo does NOT run Drop by default, so a `String` allocated inside leaks unless you opt in.
- **HdrHistogram** is the log-linear bucket histogram with **coordinated-omission backfill**. The official Rust port explicitly does not support concurrent recording; the recipe ships a per-thread-shard variant that does. Coordinated omission is the subtle bit: when your recording loop itself stalls, you miss the samples that would have shown the stall, and the percentile graph lies. The recipe walks through how `recordValueWithExpectedInterval` reconstructs the synthetic samples.

The two share more than they look. Both use the packed-atomic CAS pattern (the same trick `subms-rate-limiter` uses). Both are zero-allocation on the hot path. Both reward you the first time you measure something with one.
