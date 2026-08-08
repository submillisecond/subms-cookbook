# Merge iterator

K-way merge of sorted streams via a min-heap of stream heads. next() is one heap-pop and one heap-push.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-merge-iterator) - a `storage` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-merge-iterator = "0.9"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-merge-iterator</artifactId>
  <version>0.9.1</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-merge-iterator**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_merge_iterator`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
