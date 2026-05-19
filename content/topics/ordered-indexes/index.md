---
title: Ordered indexes
summary: Maps that keep keys sorted. The substrate underneath any range scan, prefix lookup, or sub-millisecond log query.
type: topic
order: 2
recipes:
  - recipes/subms-lsm-tree
  - recipes/subms-adaptive-radix-tree
  - recipes/subms-treap
tags:
  - data-structures
  - storage
  - ordered-indexes
---

Hash maps win on point lookups but lose every range scan. When the workload asks "everything between A and B" or "everything starting with `user:`", you need a structure that keeps keys sorted on disk or in memory.

This topic groups the three different shapes that solve that problem at sub-microsecond cost:

- **LSM tree** is the write-optimised storage layout under RocksDB, LevelDB, Cassandra, and ScyllaDB. Writes land in a memtable; flushes create immutable sorted runs on disk; reads check newest-first. A bloom filter trailer makes the negative path cheap.
- **Adaptive radix tree (ART)** is the in-memory index under DuckDB. A compact prefix tree that adapts its node layout (Node4 / Node16 / Node48 / Node256) to the fan-out at each level, getting cache-friendly density without giving up `O(k)` lookups on string keys.
- **Treap** is the simplest probabilistically-balanced BST. Each node carries a random priority; the structure keeps BST order on keys and heap order on priorities. With a good RNG you get `O(log n)` expected lookups and surprisingly little code.

You will not pick all three. You will pick one based on whether you need durability (LSM), prefix-heavy in-memory indexing (ART), or a teachable balanced map (treap). Read the others anyway; the failure modes overlap.
