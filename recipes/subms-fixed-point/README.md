# Fixed point

Exact decimal arithmetic for prices, quantities and notionals: an `i64` at nine fixed decimals with checked arithmetic, `i128` widening on multiply and divide, seven rounding modes, tick-grid rounding and scale conversion at the wire boundary. The same number in Rust and in Java, digit for digit.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-fixed-point) - a `math` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-fixed-point = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-fixed-point</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-fixed-point**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_fixed_point`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes.fixedpoint`).
- [`.subms/`](./.subms/) - the fleet-captured perf JSON + feature manifests the site renders.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
