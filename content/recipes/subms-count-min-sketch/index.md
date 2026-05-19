---
title: Count-Min Sketch
summary: Frequency-estimation sketch with conservative update and Kirsch-Mitzenmacher hashing. Always over-estimates, never under.
type: recipe
category: data-structures
repoPath: recipes/subms-count-min-sketch
order: 12
difficulty: 3
loc: 220
languages: [rust, java]
topics:
  - probabilistic-data-structures
prereqs:
  - "Power-of-two hashing"
  - "Kirsch-Mitzenmacher d-hashes-from-2 trick"
  - "Conservative update intuition"
tags:
  - data-structures
  - probabilistic-data-structures
  - frequency
perf:
  - { label: "d (depth)",    value: "5" }
  - { label: "w (width)",    value: "16384" }
  - { label: "add p99",      value: "< 100 ns" }
  - { label: "estimate p99", value: "< 100 ns" }
references:
  - { title: "Apache datasketches-java", url: "https://datasketches.apache.org/" }
  - { title: "Cormode and Muthukrishnan", url: "https://dl.acm.org/doi/10.1016/j.jalgor.2003.12.001", note: "original CMS paper" }
---

A 2D counter matrix, `d` rows by `w` columns. Each insert finds the minimum counter across the `d` cells the key hashes to, then increments only those at that minimum. Conservative update cuts over-estimation substantially versus the naive "increment all `d`" form. Queries return the minimum cell, which is `>=` the true count by construction.

Two load-bearing details:

- **Power-of-two width.** Index with `& (w-1)`, not `% w`. The recipe rounds `w` up at construction.
- **Kirsch-Mitzenmacher**. Two base hashes drive all `d` rows via `h_i = h1 + i * h2 (mod w)`. Avoids `d` independent hash computations on the hot path.

## Quality bar

**Reference impl:** Apache `datasketches-java`. Rust ecosystem is fragmented; we cross-check against `count-min-sketch-rs` on overlapping configurations.

**Sub-ms claim under:** add p99 < 100 ns; query p99 < 100 ns; over-estimation bounded by `total / w` per the conservative-update analysis (verified in unit tests: 1000 distinct keys + 1 hot key in `w=4096` produces hot-key estimate < 10).

**Not claimed:** adversarial inputs (someone crafting collisions); negative counters / non-monotone usage.
