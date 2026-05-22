---
title: HDR histogram
summary: Log-linear bucket histogram with significant-digit precision. Record p99 < 100 ns; percentile read sweeps the counter array.
type: recipe
category: observability
repoPath: recipes/subms-hdr-histogram
order: 18
level: L200
loc: 200
languages: [rust, java]
stacks: [defi]
prereqs:
  - "Log-linear bucketing"
  - "Counting with significant-digit precision"
tags:
  - observability
  - latency
  - histogram
perf:
  - { label: "record p99",     value: "< 100 ns" }
  - { label: "percentile p99", value: "< 100 us", note: "full counter sweep" }
references:
  - { title: "HdrHistogram (Java)", url: "https://github.com/HdrHistogram/HdrHistogram", note: "Gil Tene's reference" }
  - { title: "hdrhistogram (Rust)", url: "https://crates.io/crates/hdrhistogram" }
---

Each value maps to a bucket built from a major part (`floor(log2(value))`-ish) and a linear sub-part within the major's doubling range. Two parts give constant relative error inside the significant-digit precision. The recipe stores one counter per bucket; reads sweep the counter array cumulatively for any percentile.

## Quality bar

**Reference impl:** `HdrHistogram` (Java; Gil Tene); `hdrhistogram` (Rust).

**Sub-ms claim under:** record p99 < 100 ns at 3-significant-digit precision; percentile read p99 < 100 us (full counter sweep, ~17k counters).

**Not claimed:** lock-free concurrent recording (single-threaded; for the lock-free variant, shard per-thread and merge on read - separate recipe); cross-process aggregation.
