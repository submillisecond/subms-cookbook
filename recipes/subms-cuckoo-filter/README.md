# Cuckoo filter

Bloom-alternative that supports delete. Partial-key cuckoo hashing places each fingerprint in one of two candidate buckets; under load, kick a fingerprint out and re-place it.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-cuckoo-filter) - a `probabilistic` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-cuckoo-filter = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-cuckoo-filter</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-cuckoo-filter**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_cuckoo_filter`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`perf/`](./perf/) - captured `SubMsBenchSummary` JSON the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
