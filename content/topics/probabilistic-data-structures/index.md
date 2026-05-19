---
title: Probabilistic data structures
summary: Approximate answers at fixed memory cost. Trade exactness for orders of magnitude in space and latency.
type: topic
order: 1
recipes:
  - recipes/subms-bloom-filter
  - recipes/subms-hyperloglog
  - recipes/subms-count-min-sketch
  - recipes/subms-cuckoo-filter
tags:
  - data-structures
  - probabilistic-data-structures
---

A probabilistic data structure gives up exactness for a bounded resource budget. The trade is always the same: pick the false-positive rate (or the cardinality error, or the over-estimation bound), get a fixed-memory structure whose probes are a handful of hash mixes and bit reads.

The four recipes in this topic share more than a category. They share a hash family (FNV-1a 64-bit, with the same double-hashing extension), a register layout philosophy (pack densely; do not waste bytes per slot), and the same teaching arc:

- **bloom filter** answers *is this member?* with an asymmetric "definitely not" / "probably yes". Fixed bit array, `k` probes, no deletes.
- **hyperloglog** answers *how many distinct values?* with a 1-2% standard error at ~16 KB. Fixed register array; counts leading zeros in each hash.
- **count-min sketch** answers *how often did this value appear?* with a bounded over-estimation. Fixed 2D counter matrix; reads the minimum.
- **cuckoo filter** answers *is this member?* like a bloom but supports delete and trades larger items for a higher load factor.

If you are building latency-sensitive analytics, observability, or storage, you will end up reaching for one of these. Read them in order; the toolkit they share will save you from reinventing the bad version of every one.
