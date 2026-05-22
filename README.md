# subms-cookbook

The working notebook behind [submillisecond.com](https://submillisecond.com).
16 dual-language Rust + Java recipes, 3 Java primers, curated stacks,
all writeups + perf JSON, and the discovery CLI - all in one repo so one
`git clone` is enough to walk the whole library.

[![ci](https://github.com/submillisecond/subms-cookbook/actions/workflows/ci.yml/badge.svg)](https://github.com/submillisecond/subms-cookbook/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue.svg?style=flat-square)](#license)

## What's here

```
subms-cookbook/
  recipes/                   16 dual-language reusable libraries (code + writeup co-located)
    subms-bloom-filter/
      rust/                    crates.io: subms-bloom-filter
      java/                    Maven Central: com.submillisecond.recipes:subms-bloom-filter
      index.md                 cookbook page (intro + quality bar)
      rust.md                  Rust walkthrough
      java.md                  Java walkthrough
      perf/{rust.json, java.json}
    subms-lsm-tree/
    subms-spsc-ring-buffer/
    ...
  primers/                   3 Java walkthroughs (code + writeup co-located)
    java/
      subms-java21-virtual-threads/{pom.xml, src/, index.md, java.md, perf/}
      subms-zgc/
      subms-spring-boot-virtual-threads/
  stacks/                    application-domain blueprints (DeFi, ...)
    defi/
      index.md
      amm/index.md
      liquidation/index.md
      price-oracle/index.md
  cli/                       @submillisecond/subms - npm CLI to enumerate cookbook artefacts
  _archive/                  preserved-but-unrendered material (pre-migration topic intros, etc.)
```

## Recipes vs primers vs stacks

| Type | Lives at | Published | Purpose |
|---|---|---|---|
| **Recipe** | `recipes/<name>/` | crates.io + Maven Central | A reusable library - a data structure or primitive that other code can depend on. |
| **Primer** | `primers/<lang>/<name>/` | Not published | A focused walkthrough of a language feature or tool (virtual threads, ZGC, Spring Boot 4). Read it; don't `cargo add` it. |
| **Stack** | `content/stacks/<name>/` | - | An application-domain blueprint (DeFi, HFT pipeline, OLTP backend) composed of components that cite recipes + primers as ingredients. |

All 16 recipes hit p99 < 1 ms on their stated workload, measured via the
[`subms`](https://github.com/submillisecond/subms) perf harness. Every
recipe ships >= 90% line coverage and a documented quality-bar contract
(reference impl, claim conditions, non-claims).

## Recipe index

| Recipe | Category | One-liner |
|---|---|---|
| [`subms-bloom-filter`](recipes/subms-bloom-filter/) | probabilistic | FNV-1a + double hashing, ~10 bits/key, k=7 |
| [`subms-cuckoo-filter`](recipes/subms-cuckoo-filter/) | probabilistic | Bloom alternative with delete, partial-key cuckoo hashing |
| [`subms-hyperloglog`](recipes/subms-hyperloglog/) | probabilistic | HLL++ with sparse mode + linear counting |
| [`subms-count-min-sketch`](recipes/subms-count-min-sketch/) | probabilistic | CMS with conservative update + d-from-2 hashing |
| [`subms-lsm-tree`](recipes/subms-lsm-tree/) | ordered index | Memtable + immutable SSTables + bloom trailer |
| [`subms-adaptive-radix-tree`](recipes/subms-adaptive-radix-tree/) | ordered index | ART with Node4/16/48/256 adaptive variants |
| [`subms-treap`](recipes/subms-treap/) | ordered index | Probabilistic balanced BST (priority = hash) |
| [`subms-spsc-ring-buffer`](recipes/subms-spsc-ring-buffer/) | concurrency | SPSC wait-free, opposite-index caching, cache-line padded |
| [`subms-mpsc-queue`](recipes/subms-mpsc-queue/) | concurrency | Vyukov MPSC linked queue with dangling-tail handling |
| [`subms-rate-limiter`](recipes/subms-rate-limiter/) | scheduling | Lock-free token bucket with packed-atomic state |
| [`subms-timer-wheel`](recipes/subms-timer-wheel/) | scheduling | Hierarchical hashed timer wheel with cascade-on-overflow |
| [`subms-hdr-histogram`](recipes/subms-hdr-histogram/) | observability | Log-linear bucket histogram with CO backfill |
| [`subms-arena-allocator`](recipes/subms-arena-allocator/) | memory | Bump-pointer arena with chunked growth |
| [`subms-block-cache`](recipes/subms-block-cache/) | memory | LRU + clock-sweep, constant-time eviction |
| [`subms-merge-iterator`](recipes/subms-merge-iterator/) | storage | N-way sorted-stream merge via tournament tree |
| [`subms-segment-reader`](recipes/subms-segment-reader/) | storage | mmap-backed Kafka-style segment log, framed reader |
| [`subms-perf-gate`](recipes/subms-perf-gate/) | meta | The PR-time perf gate (docs-only recipe; the code is `subms-action-*`) |

## Primer index

| Primer | What it shows |
|---|---|
| [`subms-java21-virtual-threads`](primers/java/subms-java21-virtual-threads/) | virtual threads, record patterns, switch patterns, sequenced collections |
| [`subms-zgc`](primers/java/subms-zgc/) | ZGC vs G1 vs generational ZGC, heartbeat pause measurement under load |
| [`subms-spring-boot-virtual-threads`](primers/java/subms-spring-boot-virtual-threads/) | Spring Boot 4 with `spring.threads.virtual.enabled=true`, load driver |

## Naming

The cookbook uses a single `subms-` prefix across both ecosystems so artefacts
read consistently in dependency trees, on crates.io, and in `~/.m2`.

| Field | Recipes | Primers | Harness |
|---|---|---|---|
| Cargo package | `subms-<name>` | `subms-<name>` | `subms` |
| Cargo `[lib].name` | `subms_<name>` (snake) | `subms_<name>` | `subms` |
| Maven groupId | `com.submillisecond.recipes` | `com.submillisecond.primers` | `com.submillisecond` |
| Maven artifactId | `subms-<name>` | `subms-<name>` | `subms` |
| Java package | `com.submillisecond.recipes.<x>` | `com.submillisecond.primers.<x>` | `com.submillisecond.perf` |

## Conventions

- **Recipes have no third-party runtime deps.** Recipes depend only on `std` /
  the JDK and (optionally, behind a feature) on [`subms`](https://github.com/submillisecond/subms)
  for the perf harness. Downstream consumers never pull in a surprise transitive.
- **Tests in the standard locations**: JUnit 5 in `src/test/java/` (one
  `<Class>Test.java` per production class), Rust integration tests in
  `tests/<module>_tests.rs`.
- **Sub-millisecond claims are asserted in tests**. A regression on the
  headline p99 number fails CI; it doesn't quietly degrade.
- **Cross-recipe dependencies** (e.g. `subms-lsm-tree` -> `subms-bloom-filter`)
  use path deps inside this repo and version deps for downstream consumers,
  matching the standard "release each crate independently" Rust pattern.

## Publishing

Each recipe + each primer artefact publishes independently. The release
flow is per-artefact: bump version, tag `<artefact>-vX.Y.Z`, the release
workflow handles the rest.

- **Rust** -> crates.io via the per-recipe `release.yml` workflow.
- **Java** -> Maven Central (Sonatype Central portal) via the same.

Primers do not publish to any registry - read them in the repo or on
submillisecond.com.

## How this connects

| repo | role |
|---|---|
| [`subms`](https://github.com/submillisecond/subms) | The harness library every recipe depends on. |
| [`subms-cookbook`](https://github.com/submillisecond/subms-cookbook) | This repo. The content + recipe code + CLI. |
| [`subms-ui`](https://github.com/submillisecond/subms-ui) | The Next.js site that renders `content/` at build time. |
| [`subms-action-*`](https://github.com/submillisecond/subms-actions) | The PR-time perf gate. Each recipe's CI uses these to defend its p99 number. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs against in-flight recipes are
welcome but please open an issue first while the cookbook is pre-1.0 - it
saves rework if a recipe's shape is mid-iteration.

## License

Dual-licensed under your choice of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

SPDX: `MIT OR Apache-2.0`.
