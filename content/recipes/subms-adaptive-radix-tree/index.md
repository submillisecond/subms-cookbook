---
title: Adaptive Radix Tree (ART)
summary: Byte-trie with adaptive node size. Small nodes hold up to 4 children in an inline array; once a 5th child is needed, the node grows to a full 256-way dispatch.
type: recipe
category: data-structures
repoPath: recipes/subms-adaptive-radix-tree
order: 19
difficulty: 3
loc: 250
languages: [rust, java]
topics:
  - ordered-indexes
prereqs:
  - "Tries / radix trees"
  - "Linear vs map-based dispatch"
  - "Adaptive node-size tradeoff"
tags:
  - data-structures
  - ordered-indexes
  - radix-tree
perf:
  - { label: "lookup p99",  value: "< 1 us", note: "10k mixed-length keys" }
  - { label: "insert p99",  value: "< 1 us" }
references:
  - { title: "Original ART paper", url: "https://db.in.tum.de/~leis/papers/ART.pdf" }
  - { title: "DuckDB uses ART for its primary index" }
---

A byte-trie with adaptive node-size selection. Small nodes (<= 4 children) hold a 4-slot array of `(byte, child_ptr)` pairs and scan linearly. Once a node would need a 5th child, it grows to a hash-map-backed full node.

This cookbook take focuses on the load-bearing idea (adapt storage to fan-out) and omits the Node16 / Node48 in-between sizes and the path-compression optimisation from the original ART paper. Both are documented as future-work; the current implementation is correct under arbitrary keys.

## Quality bar

**Reference impl:** the ART paper (Leis et al., 2013); `art-tree` (Rust).

**Sub-ms claim under:** lookup p99 < 1 ms; insert p99 < 1 ms at 10k mixed-length keys.

**Not claimed:** memory parity with the full Node4/Node16/Node48/Node256 paper variant (we use Node4 + 256-way HashMap); path compression (each level is one byte of depth; long keys produce long chains).
