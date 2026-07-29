# SPSC ring buffer

Wait-free single-producer single-consumer ring. Padded counters and opposite-index caching keep enqueue and dequeue sub-microsecond at any contention you can give a 2-core link.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-spsc-ring-buffer) - a `concurrency` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-spsc-ring-buffer = "0.5"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-spsc-ring-buffer</artifactId>
  <version>0.5.2</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-spsc-ring-buffer**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_spsc_ring_buffer`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
