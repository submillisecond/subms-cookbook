# Seqlock

Single-writer, many-reader snapshot publication over a fixed image of 64-bit words. The writer never blocks; a reader that collides with a write retries. No lock on either side.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-seqlock) - a `concurrency` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-seqlock = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-seqlock</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-seqlock**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_seqlock`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes.seqlock`).
- [`.subms/`](./.subms/) - the fleet-captured perf JSON + feature manifests the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
