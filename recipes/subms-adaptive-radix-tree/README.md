# Adaptive Radix Tree (ART)

Byte-trie with adaptive node size. Small nodes hold up to 4 children in an inline array; once a 5th child is needed, the node grows to a full 256-way dispatch.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-adaptive-radix-tree) - a `ordered-index` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-adaptive-radix-tree = "0.8"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-adaptive-radix-tree</artifactId>
  <version>0.8.1</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-adaptive-radix-tree**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_adaptive_radix_tree`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
