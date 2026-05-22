---
title: Archived topic intros (pre-2026-05-20 IA migration)
type: archive
archived_from: content/topics/
archived_at: 2026-05-20
---

This file preserves the intro prose from the six theme-cluster topic pages
that were removed when the cookbook IA migrated from flat topics to
stacks + components plus a theme-filter on the cookbook index.

These intros are good writing - reach for them when authoring the
category-filter landing copy in subms-ui, or when a stack writeup needs a
thematic preamble.

---

## Probabilistic data structures

> Approximate answers at fixed memory cost. Trade exactness for orders of
> magnitude in space and latency.

Recipes: subms-bloom-filter, subms-hyperloglog, subms-count-min-sketch,
subms-cuckoo-filter.

A probabilistic data structure gives up exactness for a bounded resource
budget. The trade is always the same: pick the false-positive rate (or the
cardinality error, or the over-estimation bound), get a fixed-memory
structure whose probes are a handful of hash mixes and bit reads.

The four recipes in this topic share more than a category. They share a
hash family (FNV-1a 64-bit, with the same double-hashing extension), a
register layout philosophy (pack densely; do not waste bytes per slot),
and the same teaching arc:

- **bloom filter** answers *is this member?* with an asymmetric "definitely not" / "probably yes". Fixed bit array, `k` probes, no deletes.
- **hyperloglog** answers *how many distinct values?* with a 1-2% standard error at ~16 KB. Fixed register array; counts leading zeros in each hash.
- **count-min sketch** answers *how often did this value appear?* with a bounded over-estimation. Fixed 2D counter matrix; reads the minimum.
- **cuckoo filter** answers *is this member?* like a bloom but supports delete and trades larger items for a higher load factor.

If you are building latency-sensitive analytics, observability, or storage,
you will end up reaching for one of these. Read them in order; the toolkit
they share will save you from reinventing the bad version of every one.

---

## Ordered indexes

> Maps that keep keys sorted. The substrate underneath any range scan,
> prefix lookup, or sub-millisecond log query.

Recipes: subms-lsm-tree, subms-adaptive-radix-tree, subms-treap.

Hash maps win on point lookups but lose every range scan. When the workload
asks "everything between A and B" or "everything starting with `user:`",
you need a structure that keeps keys sorted on disk or in memory.

This topic groups the three different shapes that solve that problem at
sub-microsecond cost:

- **LSM tree** is the write-optimised storage layout under RocksDB, LevelDB, Cassandra, and ScyllaDB. Writes land in a memtable; flushes create immutable sorted runs on disk; reads check newest-first. A bloom filter trailer makes the negative path cheap.
- **Adaptive radix tree (ART)** is the in-memory index under DuckDB. A compact prefix tree that adapts its node layout (Node4 / Node16 / Node48 / Node256) to the fan-out at each level, getting cache-friendly density without giving up `O(k)` lookups on string keys.
- **Treap** is the simplest probabilistically-balanced BST. Each node carries a random priority; the structure keeps BST order on keys and heap order on priorities. With a good RNG you get `O(log n)` expected lookups and surprisingly little code.

You will not pick all three. You will pick one based on whether you need
durability (LSM), prefix-heavy in-memory indexing (ART), or a teachable
balanced map (treap). Read the others anyway; the failure modes overlap.

---

## Concurrent queues

> Cross-thread links that do not block. The wait-free primitives every
> event loop and actor system ride on top of.

Recipes: subms-spsc-ring-buffer, subms-mpsc-queue.

Two threads pass values without locks. The producer writes; the consumer
reads; both stay on different cores; nothing blocks; the link runs at
memory-bandwidth speeds.

Get the memory ordering wrong and the consumer reads garbage. Get the cache
layout wrong and the throughput collapses to 1/10th because the producer's
writes invalidate the consumer's read line on every iteration (false
sharing). Both recipes in this topic exist because the textbook
implementations of "lock-free queue" routinely ship with one of those bugs.

- **SPSC ring buffer** is the single-producer single-consumer link. Wait-free in both directions. Cache-line padding between the head and tail counters. Opposite-index caching so the consumer only re-reads the producer's tail when it has caught up. Used in Aeron, the Disruptor, every audio engine.
- **MPSC linked queue** is Vyukov's classic. Producers CAS to enqueue at the tail; the consumer drains by following `next` pointers. The dangling-tail window between the CAS and the link write is the load-bearing detail; everyone reinvents the buggy version that ignores it.

Read SPSC first - it teaches the memory-ordering vocabulary. Read MPSC
next - it teaches what happens when many producers contend.

---

## Scheduling and time

> Time-driven gates. Hierarchical timer wheels and lock-free token buckets
> that stay sub-microsecond at any contention.

Recipes: subms-timer-wheel, subms-rate-limiter.

Every long-lived system eventually needs two things: a way to fire
callbacks after a delay, and a way to throttle a stream of work to a target
rate. The naive implementations of each are obvious traps - a
`PriorityQueue` of timers becomes the system bottleneck under load, and a
token bucket guarded by a mutex serialises every request through one
contended lock.

The two recipes here are the canonical fixes:

- **Hierarchical timer wheel** trades the priority-queue's `O(log n)` for `O(1)` schedule and cancel. A wheel of buckets advances one tick at a time; longer timers cascade through multiple levels (Varghese and Lauck, 1987). Netty, Kafka, and the Linux kernel all ride on a wheel; Netty's is a single-level approximation that degrades for long-range timers - the hierarchical version doesn't.
- **Lock-free token bucket** packs `(tokens_x256, last_refill_ns)` into a single atomic word and CAS-loops the refill. No mutex; no doublespend; sub-microsecond `tryAcquire` even under contention. Bucket4j and `governor` ship variants of this; the packed-atomic CAS pattern reappears in HdrHistogram's recording path and the arena allocator's bump pointer.

Both recipes wear the same dial: tick granularity for the wheel, refill
rate for the bucket. Pick a granularity that matches your domain, not your
wall clock.

---

## Log and segment primitives

> Append-only segments, framed reads, and the block cache that sits in
> front. The bytes-on-disk side of every log-structured system.

Recipes: subms-segment-reader, subms-merge-iterator, subms-block-cache.

A log-structured system is a sequence of length-prefix framed records
written into rotating segment files, fronted by an in-memory block cache,
and merged on read with the next segment in the sort order. Every component
is small. None of them is hard. All of them are subtly wrong in most public
implementations.

The three recipes in this topic give you the read side of that pipeline:

- **Segment reader** opens an mmap-backed segment, walks length-prefix + CRC framed records, surfaces a typed `next()` that costs ~1 microsecond per record on a warm page cache. The cold-cache path is hardware-bound and documented as such.
- **Merge iterator** combines N sorted segment streams via a tournament tree (or a min-heap; both ship). next() is sub-microsecond up to ~16-way merges; beyond that, external sort is the right answer and the recipe says so.
- **Block cache** is the LRU + clock-sweep cache fronting the segments. Constant-time eviction; hit-path is a hash probe and a counter bump. Pairs with the segment reader to keep hot pages resident.

If you also need durability, see the LSM tree recipe - it composes these
three with an in-memory memtable and a flush policy.

---

## Memory and observability

> Per-request lifetimes and how to measure them. The bump arena and the
> percentile sketch that ride alongside every hot path.

Recipes: subms-arena-allocator, subms-hdr-histogram.

Two recipes you reach for the moment you stop tolerating allocator and
tail-latency surprises:

- **Arena allocator** is a bump pointer with chunked growth and a `reset()` per request. Allocate a scratch String, parse JSON, write a response, reset; the entire request leaves zero garbage behind. `bumpalo` is the canonical Rust crate; the JDK 22 Foreign Memory API gives you the same thing for off-heap Java. The trap (called out in the recipe): bumpalo does NOT run Drop by default, so a `String` allocated inside leaks unless you opt in.
- **HdrHistogram** is the log-linear bucket histogram with **coordinated-omission backfill**. The official Rust port explicitly does not support concurrent recording; the recipe ships a per-thread-shard variant that does. Coordinated omission is the subtle bit: when your recording loop itself stalls, you miss the samples that would have shown the stall, and the percentile graph lies. The recipe walks through how `recordValueWithExpectedInterval` reconstructs the synthetic samples.

The two share more than they look. Both use the packed-atomic CAS pattern
(the same trick `subms-rate-limiter` uses). Both are zero-allocation on the
hot path. Both reward you the first time you measure something with one.
