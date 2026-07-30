# Arena allocator

Fixed-capacity bump-pointer arena with reset() for per-request reuse. Allocate is a single cursor bump; reset is constant-time. Auto-grow is opt-in via the growable feature.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator) - a `memory` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-arena-allocator = "0.8"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-arena-allocator</artifactId>
  <version>0.8.1</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_arena_allocator`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
