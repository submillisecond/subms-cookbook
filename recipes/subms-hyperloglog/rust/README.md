# HyperLogLog - Rust

Distinct-count cardinality estimator. ~16 KB of state gives ~1% standard error up to 10^9 distinct.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-hyperloglog). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-hyperloglog = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct HyperLogLog`
- `pub fn new(precision: u32) -> Self`
- `pub fn precision(&self) -> u32`
- `pub fn register_count(&self) -> u32`
- `pub fn add(&mut self, key: &str)`
- `pub fn estimate(&self) -> f64`
- `pub fn merge(&mut self, other: &Self) -> Result<(), &'static str>`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
