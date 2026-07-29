# Rate limiter

Lock-free GCRA rate limiter using a single-atomic CAS-loop. One `tryAcquire` is one load and one CAS, even at 8-way contention.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-rate-limiter) - a `scheduling` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-rate-limiter = "0.5"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-rate-limiter</artifactId>
  <version>0.5.2</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-rate-limiter**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_rate_limiter`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
