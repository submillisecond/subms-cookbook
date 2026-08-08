# Treap

Probabilistic balanced BST. Random priorities + heap-on-priority + BST-on-key give expected O(log n) without colour bits or rebalancing factors.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-treap) - a `ordered-index` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-treap = "0.9"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-treap</artifactId>
  <version>0.9.1</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-treap**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_treap`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
