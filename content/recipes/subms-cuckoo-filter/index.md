---
title: Cuckoo filter
summary: Bloom-alternative that supports delete. Partial-key cuckoo hashing places each fingerprint in one of two candidate buckets; under load, kick a fingerprint out and re-place it.
type: recipe
category: data-structures
repoPath: recipes/subms-cuckoo-filter
order: 14
difficulty: 3
loc: 240
languages: [rust, java]
topics:
  - probabilistic-data-structures
prereqs:
  - "Bloom filter (the cookbook recipe pairs with this)"
  - "Cuckoo hashing"
  - "Partial-key derivation of the second index"
tags:
  - data-structures
  - probabilistic-data-structures
  - membership
perf:
  - { label: "fingerprint size", value: "8 bits" }
  - { label: "bucket size",      value: "4" }
  - { label: "lookup p99",       value: "< 100 ns" }
  - { label: "insert p99",       value: "< 200 ns" }
  - { label: "delete p99",       value: "< 200 ns" }
references:
  - { title: "Fan, Andersen, Kaminsky, Mitzenmacher", url: "https://www.cs.cmu.edu/~binfan/papers/conext14_cuckoofilter.pdf", note: "original cuckoo filter paper" }
  - { title: "cuckoofilter (Rust)", url: "https://crates.io/crates/cuckoofilter" }
---

Two candidate buckets per item, one byte per fingerprint, four fingerprints per bucket. Insert tries the first bucket; if full, tries the second; if also full, kicks a random fingerprint out and re-places it. Delete removes a matching fingerprint from either bucket - the bloom filter's missing trick.

The load-bearing detail: the *second* index is `i1 XOR alt(fp)`, derived from the fingerprint itself, not from a second hash of the key. That means at insert time we don't need to retain the original key - the alt-index can always be recomputed from a kicked-out fingerprint. The recipe shows the partial-key derivation in both Rust and Java.

## Quality bar

**Reference impl:** `cuckoofilter` (Rust); cross-check correctness against the original Fan et al. C++ implementation behaviour.

**Sub-ms claim under:** lookup p99 < 100 ns; insert p99 < 200 ns; delete p99 < 200 ns at 90% load factor.

**Not claimed:** behavior above ~95% load factor (eviction storms make insert unbounded; the recipe caps kicks at 500 and returns `false` rather than spinning); concurrent insert (single-threaded structure).
