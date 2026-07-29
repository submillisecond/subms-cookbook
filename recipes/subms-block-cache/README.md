# Block cache

Clock-sweep block cache with constant-time eviction. Approximates LRU; pays one hash probe + one slot bump per get.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-block-cache) - a `memory` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-block-cache = "0.5"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-block-cache</artifactId>
  <version>0.5.2</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-block-cache**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_block_cache`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
