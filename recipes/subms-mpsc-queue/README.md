# MPSC queue

Vyukov's multi-producer single-consumer linked queue. Producers swap the head; consumer walks `next` pointers. The dangling-tail window is the whole story.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-mpsc-queue) - a `concurrency` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-mpsc-queue = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-mpsc-queue</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-mpsc-queue**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_mpsc_queue`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
