---
title: Treap
summary: Probabilistic balanced BST. Random priorities + heap-on-priority + BST-on-key give expected O(log n) without colour bits or rebalancing factors.
type: recipe
category: data-structures
repoPath: recipes/subms-treap
order: 13
difficulty: 3
loc: 230
languages: [rust, java]
topics:
  - ordered-indexes
prereqs:
  - "Binary search trees + rotations"
  - "Heap invariants"
  - "Probabilistic analysis (expected height)"
tags:
  - data-structures
  - ordered-indexes
  - balanced-tree
perf:
  - { label: "lookup p99",  value: "< 1 us", note: "10k keys, deterministic seed" }
  - { label: "insert p99",  value: "< 1 us", note: "balanced via priority rotations" }
references:
  - { title: "Treap on Wikipedia", url: "https://en.wikipedia.org/wiki/Treap" }
  - { title: "Java TreeMap (red-black)", url: "https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/TreeMap.html", note: "stdlib comparison for ordered semantics" }
---

Two invariants:

- BST on keys (left subtree < node < right subtree).
- Max-heap on priorities (node priority >= children priorities).

Insertion picks a random priority for the new leaf, then rotates up while a child's priority exceeds the parent's. Deletion merges the two subtrees of the target, walking the higher-priority side down recursively. Expected height is `O(log n)`; the analysis assumes uniform priorities and is robust to adversarial *keys* but not to attacker-controlled *priorities*.

## Quality bar

**Reference impl:** Java `TreeMap` (red-black) for ordered semantics cross-check; `BTreeMap` in Rust for comparable expectations.

**Sub-ms claim under:** insert p99 < 1 ms; lookup p99 < 1 ms at 10k keys with deterministic seed.

**Not claimed:** adversarial RNG (the implementation uses a fixed LCG keyed off the constructor seed; treat the seed as trusted); concurrent insert beyond a copy-on-write epoch (this is a single-threaded structure).
