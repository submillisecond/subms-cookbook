# subms-stats

**Latency-distribution statistics for Rust + Java.** Wrap a `Vec<u64>` /
`long[]` of nanosecond readings once and read percentiles, tail shape,
robust spreads, jitter, distribution comparisons, and bootstrap
confidence intervals off it. Zero runtime dependencies; byte-equivalent
output across the two runtimes; everything operates on plain arrays.

[![crates.io](https://img.shields.io/crates/v/subms-stats.svg?logo=rust&style=flat-square)](https://crates.io/crates/subms-stats)
[![maven central](https://img.shields.io/maven-central/v/com.submillisecond.recipes/subms-stats.svg?logo=apache-maven&style=flat-square)](https://central.sonatype.com/artifact/com.submillisecond.recipes/subms-stats)
[![license](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue.svg?style=flat-square)](#license)
[![docs.rs](https://img.shields.io/badge/docs.rs-subms--stats-blue?style=flat-square&logo=rust)](https://docs.rs/subms-stats)

The deep treatment lives at
<https://submillisecond.com/cookbook/recipes/subms-stats>. This README is
the install + a one-screen tour.

## What ships

| Feature (Rust) / class (Java) | What it answers                                                  |
|-------------------------------|------------------------------------------------------------------|
| `percentiles` (always on)     | `p50` / `p99` / `p99.9` / `max`, `mean`, `stddev`                |
| `histogram`                   | 64-bucket log2 CDF (1 ns .. ~18 s) for full-distribution export  |
| `jitter`                      | Per-window CoV stability score (was the rig steady?)             |
| `tail`                        | Conditional tail expectation, Hill index, p99 / p50 fatness      |
| `robust`                      | IQR, MAD, CoV, skewness, excess kurtosis                         |
| `compare`                     | KS statistic, Cohen's d effect size                              |
| `bootstrap`                   | Reproducible bootstrap CIs for percentiles                       |

## Install

### Rust

```toml
[dependencies]
subms-stats = "0.6"
```

All features are on by default. Trim with `default-features = false` and
opt in by name.

### Java (Maven)

```xml
<dependency>
    <groupId>com.submillisecond.recipes</groupId>
    <artifactId>subms-stats</artifactId>
    <version>0.6.0</version>
</dependency>
```

JDK 21 baseline, zero transitive deps.

## Quickstart

```rust
use subms_stats::SubMsSamples;

let raw: Vec<u64> = collect_latencies_ns();
let s = SubMsSamples::new(&raw);
println!("p50 {}  p99 {}  p99.9 {}", s.p50(), s.p99(), s.p999());
```

```java
import com.submillisecond.stats.SubMsSamples;

long[] raw = collectLatenciesNs();
SubMsSamples s = SubMsSamples.of(raw);
System.out.printf("p50 %d  p99 %d  p99.9 %d%n", s.p50(), s.p99(), s.p999());
```

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.

## See also

- Recipe page: <https://submillisecond.com/cookbook/recipes/subms-stats>
- Harness: <https://github.com/submillisecond/subms>
- Cookbook: <https://submillisecond.com/cookbook>
