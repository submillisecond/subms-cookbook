# HDR histogram - Rust

Log-linear bucket histogram with significant-digit precision. Record p99 < 100 ns; percentile read sweeps the counter array.

Part of the [submillisecond.com cookbook](https://submillisecond.com/cookbook/recipes/subms-hdr-histogram). Zero external dependencies; `std` only.

## Install

```toml
[dependencies]
subms-hdr-histogram = "0.4"
```

## Quickstart

```sh
cargo test --release
cargo run --example demo
```

## Public API

- `pub struct HdrHistogram`
- `pub fn new(significant_digits: u32) -> Self`
- `pub fn count(&self) -> u64`
- `pub fn max(&self) -> u64`
- `pub fn record(&mut self, value: u64)`
- `pub fn value_at_percentile(&self, q: f64) -> u64`
- `pub fn sub_count(&self) -> u32`

## Files

- `src/lib.rs` - implementation.
- `tests/` - integration tests; correctness, edge cases, property/stress.
- `examples/demo.rs` - stdout walkthrough.
- `examples/perf_main.rs` - bench entry (behind the `harness` feature).

## License

Dual-licensed under MIT OR Apache-2.0.
