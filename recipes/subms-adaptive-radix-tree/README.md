# Adaptive Radix Tree (ART)

Byte-keyed trie with adaptive node size. Child storage moves up a four-rung ladder as fan-out grows - `Node4` and `Node16` scan short key arrays, `Node48` indexes 48 slots through a 256-byte table, `Node256` indexes the byte directly - and path compression collapses a run of single-child bytes into one node's prefix.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-adaptive-radix-tree) - a `ordered-index` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-adaptive-radix-tree = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-adaptive-radix-tree</artifactId>
  <version>0.10.0</version>
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
- [`.subms/perf/`](./.subms/perf/) - the fleet-captured latency JSON the site renders.
- [`.subms/features/`](./.subms/features/) - the per-feature cost classification.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
