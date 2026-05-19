---
title: HyperLogLog
summary: Distinct-count cardinality estimator. ~16 KB of state gives ~1% standard error up to 10^9 distinct.
type: recipe
category: data-structures
repoPath: recipes/subms-hyperloglog
order: 11
difficulty: 3
loc: 250
languages: [rust, java]
topics:
  - probabilistic-data-structures
prereqs:
  - "FNV-1a hashing"
  - "Leading-zero count and bit twiddling"
  - "Linear counting (the bias correction at low cardinality)"
tags:
  - data-structures
  - probabilistic-data-structures
  - cardinality
perf:
  - { label: "precision",     value: "p=14",       note: "m = 16384 registers; ~1% standard error" }
  - { label: "memory",        value: "~16 KB",     note: "one byte per register (6-bit values fit in a byte)" }
  - { label: "add p99",       value: "< 100 ns" }
  - { label: "estimate p99",  value: "< 100 us",   note: "full m-register scan" }
references:
  - { title: "Apache DataSketches (Java)", url: "https://datasketches.apache.org/", note: "modern reference; mergeable, well-tested" }
  - { title: "Flajolet et al. HLL paper",  url: "https://stefanheule.com/papers/edbt13-hyperloglog.pdf", note: "HLL++ extensions" }
---

A probabilistic distinct-count sketch. Hash each item; the top `p` bits pick a register; the leading-zero count of the remaining `64-p` bits goes into the register (max wins). The estimate is `alpha * m^2 / sum(2^-r_i)`, with linear counting at low cardinality.

Two load-bearing details:

- **Hash quality matters.** FNV-1a alone clusters short sequential keys; a SplitMix64 finalizer fixes it. Both Rust and Java implementations apply the finalizer before extracting the bucket and the leading-zero count.
- **Linear counting at small cardinality.** The raw harmonic-mean estimator is wildly biased for `n < 2.5m`. The recipe checks for zero-valued registers and switches to `-m * ln(zeros/m)`, which is tight in that regime.

## Quality bar

**Reference impl:** Apache `datasketches-java`. The Rust ecosystem is fragmented; the recipe behaviour is validated against `hyperloglogplus` on overlapping precisions.

**Sub-ms claim under:** add p99 < 100 ns; estimate p99 < 100 us at m = 16384; error < 2% at 10k distinct, < 3% at 10k-merged across two HLLs (Rust unit-test verified).

**Not claimed:** streaming workloads that mutate precision/hash mid-stream; merge across mismatched precision (we detect and refuse).
