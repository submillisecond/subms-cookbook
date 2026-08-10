# Timer wheel

Single-level hashed timer wheel. O(1) schedule, cancel and reschedule; tick fires every timer with rounds=0 in the current bucket.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-timer-wheel) - a `scheduling` recipe. Rust + Java, byte-equivalent JSON contract, zero runtime dependencies (`std` / JDK only; the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-timer-wheel = "0.10"
```

```xml
<!-- Maven -->
<dependency>
  <groupId>com.submillisecond.recipes</groupId>
  <artifactId>subms-timer-wheel</artifactId>
  <version>0.10.0</version>
</dependency>
```

## Docs, design, and measured benchmarks

The full writeup - implementation walkthrough, design tradeoffs, the quality-bar
contract (reference impl / claim conditions / non-claims), and the p99 numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-timer-wheel**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_timer_wheel`).
- [`java/`](./java/) - JDK 21 (`com.submillisecond.recipes`).
- [`.subms/perf/`](./.subms/perf/) - captured bench JSON the site renders.
- [`.subms/features/`](./.subms/features/) - per-feature perf classification.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
